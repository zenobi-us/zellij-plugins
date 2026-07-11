#!/usr/bin/env python3
import fcntl
import os
import pty
import struct
import sys
import termios

pid, fd = pty.fork()
if pid == 0:
    os.execvp(sys.argv[1], sys.argv[1:])

fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
log_path = os.environ.get("PTY_LOG")
log = open(log_path, "wb") if log_path else None
while True:
    try:
        data = os.read(fd, 4096)
        if log:
            log.write(data)
            log.flush()
    except OSError:
        break
