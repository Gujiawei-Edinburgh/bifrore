import os
import sys
from importlib.resources import files


def main():
    binary = files("tenon_oss").joinpath("bin/tenon-oss")
    binary_path = os.fspath(binary)
    try:
        current_mode = os.stat(binary_path).st_mode
        os.chmod(binary_path, current_mode | 0o111)
    except OSError:
        pass
    os.execv(binary_path, [binary_path, *sys.argv[1:]])
