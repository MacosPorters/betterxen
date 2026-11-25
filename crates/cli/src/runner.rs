// SPDX-License-Identifier: GPL-3.0-or-later

use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use xenora_loader::startup::{StartupEvent, StartupLayout, run_synthetic_startup_with_observer};
use xenora_loader::{
    ExitReason, ImportRegistry, Register, ServicesState, SystemClock, register_standard_services,
};
use xenora_metal::NativeCompute;
use xenora_shader::{Architecture, PIXEL_ENTRY_POINT, VERTEX_ENTRY_POINT, compile_graphics};

#[path = "startup_report.rs"]
mod startup_report;
use startup_report::{Events, JOURNAL_ERROR_EXIT, Journal, Termination, WATCHDOG_EXIT};

const FIXTURE: &[u8] = include_bytes!("../../../fixtures/pe/trusted-runner.exe");
const SERVICES_FIXTURE: &[u8] = include_bytes!("../../../fixtures/pe/runtime-services.exe");
const WALL_LIMIT: Duration = Duration::from_secs(15);
const DEFAULT_STEPS: usize = 100;
const MAX_STEPS: usize = 10_000;
const IMAGE_BASE: u64 = 0x150000000;
const TICKET_ADDRESS: u64 = IMAGE_BASE + 0x3000;
const TICKET: u64 = 17;
const DRAW_RESULT: u64 = 0x42;
const EXTENT: [usize; 2] = [4, 3];
const EXPECTED_PIXEL: [u8; 4] = [255, 0, 0, 255];
const USAGE: &str = "usage: betterxen run --pe <trusted-fixture> --experimental [--trace] [--metal-hud] [--max-steps <1..=10000>] [--startup-report <path>]";

struct Options {
    path: PathBuf,
    max_steps: usize,
    worker: bool,
    trace: bool,
    hud: bool,
    report: Option<PathBuf>,
    journal: Option<PathBuf>,
}

impl Options {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let mut path = None;
        let mut max_steps = None;
        let mut report = None;
        let mut journal = None;
        let (mut experimental, mut trace, mut hud, mut worker) = (false, false, false, false);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--pe") if path.is_none() => {
                    path = Some(PathBuf::from(args.next().ok_or("missing --pe path")?));
                }
                Some("--max-steps") if max_steps.is_none() => {
                    max_steps = Some(
                        args.next()
                            .and_then(|arg| arg.to_str()?.parse::<usize>().ok())
                            .filter(|steps| (1..=MAX_STEPS).contains(steps))
                            .ok_or("--max-steps must be in 1..=10000")?,
                    );
                }
                Some("--startup-report") if report.is_none() => {
                    report = Some(PathBuf::from(
                        args.next().ok_or("missing --startup-report path")?,
                    ));
                }
                Some("--startup-journal") if journal.is_none() => {
                    journal = Some(PathBuf::from(
                        args.next().ok_or("missing worker journal path")?,
                    ));
                }
                Some("--experimental") if !experimental => experimental = true,
                Some("--trace") if !trace => trace = true,
                Some("--metal-hud") if !hud => hud = true,
                Some("--worker") if !worker => worker = true,
                _ => return Err(format!("unknown or repeated run argument {arg:?}")),
            }
        }
        if !experimental {
            return Err("run requires --experimental".into());
        }
        if (worker && report.is_some()) || (!worker && journal.is_some()) {
            return Err("invalid startup report option for this process".into());
        }
        Ok(Self {
            path: path.ok_or("missing --pe path")?,
            max_steps: max_steps.unwrap_or(DEFAULT_STEPS),
            worker,
            trace,
            hud,
            report,
            journal,
        })
    }
}

pub fn command(args: impl IntoIterator<Item = OsString>) -> ExitCode {
    let options = match Options::parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let result = if options.worker {
        worker_command(&options)
    } else {
        launch_worker(&options)
    };
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("run failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn launch_worker(options: &Options) -> Result<ExitCode, Box<dyn Error>> {
    let journal = options
        .report
        .as_ref()
        .map(|_| Journal::create())
        .transpose()?;
    let executable = std::env::current_exe()?;
    let destination = options
        .report
        .as_ref()
        .map(|path| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|error| format!("could not create startup report: {error}"))
        })
        .transpose()?;
    let started = Instant::now();
    let mut command = Command::new(executable);
    command
        .args([
            "run",
            "--experimental",
            "--trace",
            "--metal-hud",
            "--worker",
        ])
        .arg("--max-steps")
        .arg(options.max_steps.to_string())
        .arg("--pe")
        .arg(&options.path)
        .stdin(Stdio::null());
    if let Some(journal) = &journal {
        command.arg("--startup-journal").arg(journal.path());
    }
    let termination = command
        .spawn()
        .and_then(|child| supervise(child, started, WALL_LIMIT));
    if let (Some(journal), Some(destination)) = (&journal, destination)
        && let Err(error) = journal.finish(destination, &termination)
    {
        eprintln!("could not write startup report: {error}");
        if matches!(&termination, Ok(Termination::Exited(status)) if status.success()) {
            return Ok(ExitCode::FAILURE);
        }
    }
    match termination? {
        Termination::Exited(status) => match status.code() {
            Some(code) => Ok(ExitCode::from(u8::try_from(code)?)),
            None => Err(format!("worker terminated: {status}").into()),
        },
        Termination::Deadline => {
            Err("experimental worker exceeded its 15 second wall deadline".into())
        }
    }
}

fn supervise(mut child: Child, started: Instant, limit: Duration) -> std::io::Result<Termination> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Termination::Exited(status)),
            Ok(None) if started.elapsed() < limit => {
                std::thread::sleep(Duration::from_millis(10));
            }
            status => {
                let _ = child.kill();
                child.wait()?;
                status?;
                return Ok(Termination::Deadline);
            }
        }
    }
}

fn worker_command(options: &Options) -> Result<ExitCode, Box<dyn Error>> {
    let mut events = match options.journal.as_deref().map(Events::open).transpose() {
        Ok(events) => events,
        Err(error) => {
            eprintln!("could not open startup journal: {error}");
            return Ok(ExitCode::from(JOURNAL_ERROR_EXIT));
        }
    };
    let result = run_worker(options, events.as_mut());
    if let Some(events) = events.as_mut() {
        if let Err(error) = &result {
            events.failure(error.as_ref());
        }
        if let Some(error) = events.write_error() {
            if let Err(error) = result {
                eprintln!("run failed: {error}");
            }
            eprintln!("could not write startup journal: {error}");
            return Ok(ExitCode::from(JOURNAL_ERROR_EXIT));
        }
    }
    result.map(|()| ExitCode::SUCCESS)
}

fn verify_fixture(path: &Path) -> Result<&'static [u8], Box<dyn Error>> {
    let valid_metadata =
        |metadata: fs::Metadata| metadata.is_file() && metadata.len() == FIXTURE.len() as u64;
    let rejection =
        "execution denied: input must match an embedded trusted PE fixture byte for byte";
    // Check size before opening and again on the descriptor. Bound the read
    // even if another process changes the file after the metadata checks.
    if !valid_metadata(fs::metadata(path)?) {
        return Err(rejection.into());
    }
    let file = File::open(path)?;
    if !valid_metadata(file.metadata()?) {
        return Err(rejection.into());
    }
    let mut bytes = Vec::with_capacity(FIXTURE.len() + 1);
    file.take(FIXTURE.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    [FIXTURE, SERVICES_FIXTURE]
        .into_iter()
        .find(|fixture| *fixture == bytes)
        .ok_or_else(|| rejection.into())
}

fn run_worker(options: &Options, mut events: Option<&mut Events>) -> Result<(), Box<dyn Error>> {
    std::thread::Builder::new()
        .name("xenora-deadline".into())
        .spawn(|| {
            std::thread::sleep(WALL_LIMIT);
            // The supervisor also enforces this deadline. The worker retains
            // its own deadline if it is invoked directly.
            std::process::exit(i32::from(WATCHDOG_EXIT));
        })?;
    let fixture = verify_fixture(&options.path)?;
    let (name, ticket, bound_imports) = if fixture == SERVICES_FIXTURE {
        ("runtime-services", 23, 3)
    } else {
        ("trusted-runner", TICKET, 1)
    };
    if options.trace {
        println!(
            "trace fixture={name} policy=exact-bytes entry_step_limit={} wall_limit_ms=15000",
            options.max_steps
        );
    }

    let capture = Arc::new(Mutex::new(None));
    let draw_capture = Arc::clone(&capture);
    let mut imports = ImportRegistry::new();
    register_standard_services(
        &mut imports,
        Arc::new(ServicesState::new(Arc::new(SystemClock))),
    );
    let trace = options.trace;
    imports.register("xenora.dll", "fixture_draw", move |args, _, memory| {
        let ticket_address = args.get(0, memory).map_err(ExitReason::MemoryFault)?;
        let mut result = draw_capture
            .lock()
            .map_err(|_| ExitReason::IllegalInstruction(0xff))?;
        if result.is_some() {
            return Err(ExitReason::IllegalInstruction(0xff));
        }
        if ticket_address != TICKET_ADDRESS
            || memory
                .read_u64(ticket_address)
                .map_err(ExitReason::MemoryFault)?
                != ticket
        {
            *result = Some(Err(
                "guest draw ticket differed from the expected value".into()
            ));
            return Err(ExitReason::IllegalInstruction(0xff));
        }
        if trace {
            println!("trace import=xenora.dll!fixture_draw tls_ticket={ticket}");
        }
        let frame = draw().map_err(|error| error.to_string());
        let drawn = frame.is_ok();
        *result = Some(frame);
        if drawn {
            Ok(DRAW_RESULT)
        } else {
            Err(ExitReason::IllegalInstruction(0xff))
        }
    });

    let started = Instant::now();
    // Use only the embedded snapshot. The file is solely an exact-byte gate.
    let startup = run_synthetic_startup_with_observer(
        fixture,
        StartupLayout {
            image_base: IMAGE_BASE,
            stack_top: 0x200010000,
            tls_base: 0x300000000,
        },
        imports,
        options.max_steps,
        |event| {
            if let Some(events) = events.as_deref_mut() {
                events.stage(match &event {
                    StartupEvent::Parsed { .. } => "parsed",
                    StartupEvent::Mapped { .. } => "mapped",
                    StartupEvent::Relocated { .. } => "relocated",
                    StartupEvent::ImportsBound { .. } => "imports_bound",
                    StartupEvent::TlsCompleted { .. } => "tls_completed",
                    StartupEvent::Entering { .. } => "entry",
                });
            }
            if options.trace {
                trace_startup(event);
            }
        },
    )?;
    let host_ms = started.elapsed().as_secs_f64() * 1000.0;
    let result = capture
        .lock()
        .map_err(|_| "fixture draw result lock failed")?
        .take();
    let exit = startup.execution.exit_reason;
    let rax = startup.execution.final_state.get(Register::Rax);
    if exit == ExitReason::Halt
        && let Some(events) = events.as_deref_mut()
    {
        events.stage("exit");
    }
    if options.trace {
        println!(
            "trace stage=exit entry_steps={} exit={exit:?} rax={rax:#x}",
            startup.execution.instructions_executed,
        );
    }
    if let Some(Err(error)) = &result {
        return Err(format!("fixture draw failed: {error}").into());
    }
    if exit != ExitReason::Halt {
        return Err(exit.into());
    }
    if startup.relocations.applied_dir64 != 4
        || startup.bound_imports != bound_imports
        || startup.tls_callbacks != 1
        || rax != DRAW_RESULT
    {
        return Err("trusted fixture startup or result differs from the expected values".into());
    }
    let frame = result.ok_or("guest halted without a draw")??;
    if let Some(events) = events {
        events.stage("framebuffer");
    }
    if options.trace {
        println!(
            "trace stage=framebuffer extent={}x{} verified_pixels={}",
            EXTENT[0],
            EXTENT[1],
            EXTENT[0] * EXTENT[1],
        );
    }
    if options.hud {
        let gpu_time = frame.gpu_seconds.map_or_else(
            || "unavailable".to_owned(),
            |seconds| format!("{:.3}", seconds * 1000.0),
        );
        println!(
            "Metal HUD | device={} | offscreen={}x{} | draw_calls=1 | verified_pixels={} | host_render_readback_ms={:.3} | host_startup_ms={host_ms:.3} | gpu_command_ms={gpu_time}",
            frame.device,
            EXTENT[0],
            EXTENT[1],
            EXTENT[0] * EXTENT[1],
            frame.host_render_readback_ms,
        );
        let metrics = startup.dbt_metrics;
        println!(
            "DBT | entry_steps={} | native_steps_tls_and_entry={} | blocks_compiled={} | native_bytes={} | native_dispatches={} | service_calls={} | interpreter_fallbacks={}",
            startup.execution.instructions_executed,
            metrics.instructions_executed,
            metrics.blocks_compiled,
            metrics.native_bytes_emitted,
            metrics.dbt_dispatches,
            metrics.service_calls,
            metrics.interpreter_fallbacks,
        );
    }
    println!("Trusted fixture completed one verified offscreen Metal draw.");
    Ok(())
}

fn trace_startup(event: StartupEvent) {
    match event {
        StartupEvent::Parsed {
            sections,
            entry_rva,
        } => {
            println!("trace stage=parsed sections={sections} entry_rva={entry_rva:#x}");
        }
        StartupEvent::Mapped { entry_va } => {
            println!("trace stage=mapped base={IMAGE_BASE:#x} entry_va={entry_va:#x}");
        }
        StartupEvent::Relocated { applied_dir64 } => {
            println!("trace stage=relocated dir64_applied={applied_dir64}");
        }
        StartupEvent::ImportsBound { count } => {
            println!("trace stage=imports_bound bound_imports={count}");
        }
        StartupEvent::TlsCompleted {
            callbacks,
            native_steps,
        } => {
            println!(
                "trace stage=tls_completed tls_callbacks={callbacks} native_steps={native_steps}"
            );
        }
        StartupEvent::Entering { step_limit } => {
            println!("trace stage=entry step_limit={step_limit}");
        }
    }
}

struct Frame {
    device: String,
    host_render_readback_ms: f64,
    gpu_seconds: Option<f64>,
}

fn draw() -> Result<Frame, Box<dyn Error>> {
    let shader = compile_graphics(
        Architecture::Gfx1030,
        include_bytes!("../../../fixtures/rdna2/x9-constant-vs.bin"),
        include_bytes!("../../../fixtures/rdna2/x9-constant-ps.bin"),
    )?;
    let gpu = NativeCompute::new()?;
    let started = Instant::now();
    let (pixels, gpu_seconds) = gpu.render_rgba8_profiled(
        &shader.msl,
        [VERTEX_ENTRY_POINT, PIXEL_ENTRY_POINT],
        &[
            [-1.0, -1.0, 0.5, 1.0],
            [3.0, -1.0, 0.5, 1.0],
            [-1.0, 3.0, 0.5, 1.0],
        ],
        &[[0.0; 4]; 3],
        EXTENT,
        [19, 43, 93, 251],
    )?;
    let host_render_readback_ms = started.elapsed().as_secs_f64() * 1000.0;
    if pixels != EXPECTED_PIXEL.repeat(EXTENT[0] * EXTENT[1]) {
        return Err("offscreen readback did not match opaque red at every pixel".into());
    }
    Ok(Frame {
        device: gpu.device_name().to_owned(),
        host_render_readback_ms,
        gpu_seconds,
    })
}
