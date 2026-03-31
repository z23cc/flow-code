"""Platform-specific compatibility: file locking (fcntl on Unix, no-op on Windows)."""

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
