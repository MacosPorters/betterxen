#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
set -eu
cd "$(dirname "$0")/.."

export MTL_DEBUG_LAYER=1
export MTL_SHADER_VALIDATION=1
export MTL_SHADER_VALIDATION_REPORT_TO_STDERR=1
export MTL_SHADER_VALIDATION_ABORT_ON_FAULT=1
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}

cargo test --locked -p xenora-metal --features gpu-tests --test native_compute --no-run
timeout 30s cargo test --locked -p xenora-metal --features gpu-tests --test native_compute -- \
    --nocapture --test-threads=1

cargo test --locked -p xenora-loader --features gpu-tests --test dbt_render --no-run
timeout 30s cargo test --locked -p xenora-loader --features gpu-tests --test dbt_render -- \
    --nocapture --test-threads=1
