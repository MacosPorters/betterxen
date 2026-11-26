import subprocess,sys
p=subprocess.Popen([sys.executable,sys.argv[1]],stdin=sys.stdin,stdout=sys.stdout,stderr=sys.stderr,start_new_session=True)
sys.exit(p.wait())
