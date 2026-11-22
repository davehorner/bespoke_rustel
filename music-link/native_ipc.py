"""Shared-memory inspection/cleanup for launchers and hardware-free probes."""
from contextlib import contextmanager
import ctypes
import ctypes.util
import errno
import mmap
import os
import re
import sys

SIZE = 4096 + 15 * 16384 * 2 * 8
POSIX_NAME = re.compile(r"/rla-([0-9a-f]{1,8})-[0-9a-f]{1,16}\Z")


def windows_mapping_exists(name):
    """Inspect without mmap's create-if-missing behavior on Windows."""
    if not name.startswith("Local\\RustelAudio-"):
        raise ValueError("Unexpected Windows lane mapping name")
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.OpenFileMappingW.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_wchar_p]
    kernel.OpenFileMappingW.restype = ctypes.c_void_p
    kernel.CloseHandle.argtypes = [ctypes.c_void_p]
    kernel.CloseHandle.restype = ctypes.c_int
    handle = kernel.OpenFileMappingW(4, False, name)  # FILE_MAP_READ; never creates.
    if not handle:
        error = ctypes.get_last_error()
        if error == 2:  # ERROR_FILE_NOT_FOUND
            return False
        raise ctypes.WinError(error)
    kernel.CloseHandle(handle)
    return True


def posix_lib():
    library = ctypes.CDLL(None, use_errno=True)
    if not hasattr(library, "shm_open"):
        library = ctypes.CDLL(ctypes.util.find_library("rt"), use_errno=True)
    # Darwin declares shm_open variadic; Apple Silicon passes variadic arguments
    # differently. Describe only its fixed parameters to ctypes on that platform.
    library.shm_open.argtypes = ([ctypes.c_char_p, ctypes.c_int] if sys.platform == "darwin"
                                else [ctypes.c_char_p, ctypes.c_int, ctypes.c_uint])
    library.shm_open.restype = ctypes.c_int
    library.shm_unlink.argtypes = [ctypes.c_char_p]
    library.shm_unlink.restype = ctypes.c_int
    return library


@contextmanager
def open_mapping(name):
    if os.name == "nt":
        if not name.startswith("Local\\RustelAudio-"):
            raise ValueError("Unexpected Windows lane mapping name")
        with mmap.mmap(-1, SIZE, tagname=name) as memory:
            yield memory
    else:
        if not POSIX_NAME.fullmatch(name):
            raise ValueError("Unexpected POSIX lane mapping name")
        library = posix_lib()
        fd = library.shm_open(name.encode(), os.O_RDWR, 0)
        if fd < 0:
            raise OSError(ctypes.get_errno(), "Cannot open lane shared memory")
        try:
            status = os.fstat(fd)
            # Darwin reports the page-rounded allocation size. The protocol
            # needs SIZE bytes, not an allocation whose size is exactly SIZE.
            if status.st_size < SIZE or status.st_uid != os.geteuid():
                raise ValueError(f"Unexpected shared-memory owner or insufficient capacity: {status.st_size} bytes")
            with mmap.mmap(fd, SIZE, flags=mmap.MAP_SHARED, prot=mmap.PROT_READ | mmap.PROT_WRITE) as memory:
                yield memory
        finally:
            os.close(fd)


def unlink_stopped_mapping(name, pid):
    """Call only after verifying that the owned producer has stopped."""
    if os.name == "nt":
        return
    match = POSIX_NAME.fullmatch(name)
    if not match or int(match[1], 16) != pid:
        raise ValueError("Refusing to unlink a mapping for a different producer")
    if posix_lib().shm_unlink(name.encode()) != 0 and ctypes.get_errno() != errno.ENOENT:
        raise OSError(ctypes.get_errno(), "Cannot unlink stopped lane mapping")
