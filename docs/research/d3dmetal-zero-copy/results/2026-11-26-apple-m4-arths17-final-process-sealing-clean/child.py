import os,sys
print('CHILD_PID='+str(os.getpid()),flush=True)
print('BACKEND_READY',flush=True)
line=sys.stdin.buffer.read(1)
if not line:sys.stderr.write('END failure: backend receipt handshake\r\n');sys.stderr.flush()
else:print('END complete',flush=True)
