"""Platform-specific compatibility: file locking and fsync (fcntl on Unix, no-op on Windows)."""

import os
import sys

try:
    import fcntl

    def _flock(f, lock_type):
        fcntl.flock(f, lock_type)

    LOCK_EX = fcntl.LOCK_EX
    LOCK_UN = fcntl.LOCK_UN
except ImportError:
    # Windows: fcntl not available, use no-op (acceptable for single-machine use)
    def _flock(f, lock_type):
        pass

    LOCK_EX = 0
    LOCK_UN = 0


def _fsync(fd: int) -> None:
    """Platform-aware fsync: uses F_FULLFSYNC on macOS for true hardware flush."""
    if sys.platform == "darwin":
        try:
            import fcntl as _fcntl
            _fcntl.fcntl(fd, _fcntl.F_FULLFSYNC)
            return
        except (ImportError, AttributeError, OSError):
            pass  # Fall through to os.fsync
    os.fsync(fd)
