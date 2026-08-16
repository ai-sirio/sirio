import json, socket, sys
sock, method = sys.argv[1], sys.argv[2]
params = dict(p.split('=',1) for p in sys.argv[3:])
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM); s.settimeout(20); s.connect(sock)
s.sendall((json.dumps({"id":"probe","method":method,"params":params})+"\n").encode())
buf=b""
while not buf.endswith(b"\n"):
    c=s.recv(65536)
    if not c: break
    buf+=c
print(buf.decode().strip())
