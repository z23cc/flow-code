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
    # Windows: fcntl not available, try msvcrt for file locking
    try:
        import msvcrt

        LOCK_EX = 1  # map to LK_NBLCK
        LOCK_UN = 2  # map to LK_UNLCK

        def _flock(f, lock_type):
            if lock_type == LOCK_EX:
                msvcrt.locking(f.fileno(), msvcrt.LK_NBLCK, 1)
            elif lock_type == LOCK_UN:
                msvcrt.locking(f.fileno(), msvcrt.LK_UNLCK, 1)

    except ImportError:
        # Non-Unix, non-Windows: no-op with warning
        import warnings
        warnings.warn("File locking unavailable on this platform; concurrent access is unprotected")

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
