#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="$ROOT_DIR/build"
RUST_DIR="$ROOT_DIR/runtime"
TENON_VERSION="${TENON_VERSION:-0.1.0}"

OS_NAME="$(uname -s)"
case "$OS_NAME" in
  Darwin|Linux)
    ;;
  *)
    echo "Unsupported OS: $OS_NAME"
    exit 2
    ;;
esac

mkdir -p "$BUILD_DIR"

platform_tag() {
  if [[ -n "${TENON_WHEEL_PLATFORM_TAG:-}" ]]; then
    echo "$TENON_WHEEL_PLATFORM_TAG"
    return
  fi
  case "$OS_NAME" in
    Darwin)
      case "$(uname -m)" in
        arm64|aarch64) echo "macosx_11_0_arm64" ;;
        *) echo "unsupported" ;;
      esac
      ;;
    Linux)
      case "$(uname -m)" in
        x86_64|amd64) echo "manylinux_2_28_x86_64" ;;
        arm64|aarch64) echo "manylinux_2_28_aarch64" ;;
        *) echo "unsupported" ;;
      esac
      ;;
    *)
      echo "unsupported"
      ;;
  esac
}

native_platform_dir() {
  case "$OS_NAME" in
    Darwin)
      case "$(uname -m)" in
        arm64|aarch64) echo "darwin-aarch64" ;;
        *) echo "unsupported" ;;
      esac
      ;;
    Linux)
      case "$(uname -m)" in
        x86_64|amd64) echo "linux-x86_64" ;;
        arm64|aarch64) echo "linux-aarch64" ;;
        *) echo "unsupported" ;;
      esac
      ;;
    *)
      echo "unsupported"
      ;;
  esac
}

oss_binary_name() {
  local platform
  platform="$(native_platform_dir)"
  if [[ "$platform" == "unsupported" ]]; then
    echo "unsupported"
    return
  fi
  echo "tenon-oss-${TENON_VERSION}-${platform}"
}

build_tenon_oss_binary() {
  echo "Building tenon-oss binary..."
  local binary_name
  binary_name="$(oss_binary_name)"
  if [[ "$binary_name" == "unsupported" ]]; then
    echo "Unsupported platform for tenon-oss binary: $OS_NAME/$(uname -m)"
    exit 7
  fi
  (cd "$RUST_DIR" && cargo build --release -p tenon-oss)
  cp "$RUST_DIR/target/release/tenon-oss" "$BUILD_DIR/$binary_name"
  echo "tenon-oss binary: $BUILD_DIR/$binary_name"
}

build_tenon_oss_pypi() {
  echo "Building tenon-oss PyPI wheel..."
  local platform binary_name wheel_stage package_dir
  platform="$(platform_tag)"
  if [[ "$platform" == "unsupported" ]]; then
    echo "Unsupported platform for tenon-oss PyPI wheel: $OS_NAME/$(uname -m)"
    exit 8
  fi

  build_tenon_oss_binary
  binary_name="$(oss_binary_name)"
  wheel_stage="$BUILD_DIR/tenon-oss-pypi-stage"
  package_dir="$wheel_stage/tenon_oss"
  rm -f "$BUILD_DIR"/tenon_oss-*.whl
  rm -rf "$wheel_stage"
  mkdir -p "$package_dir/bin"

  cp "$ROOT_DIR/scripts/tenon-oss-pypi/tenon_oss/__init__.py" "$package_dir/__init__.py"
  cp "$ROOT_DIR/scripts/tenon-oss-pypi/tenon_oss/cli.py" "$package_dir/cli.py"
  cp "$BUILD_DIR/$binary_name" "$package_dir/bin/tenon-oss"
  chmod 755 "$package_dir/bin/tenon-oss"

  cat > "$wheel_stage/setup.py" <<EOF2
from setuptools import setup
from wheel.bdist_wheel import bdist_wheel


class PlatformWheel(bdist_wheel):
    def finalize_options(self):
        super().finalize_options()
        self.root_is_pure = False

    def get_tag(self):
        return "py3", "none", self.plat_name


setup(
    name="tenon-oss",
    version="${TENON_VERSION}",
    description="TENON standalone OSS executable",
    packages=["tenon_oss"],
    package_data={"tenon_oss": ["bin/tenon-oss"]},
    entry_points={"console_scripts": ["tenon-oss=tenon_oss.cli:main"]},
    include_package_data=True,
    zip_safe=False,
    cmdclass={"bdist_wheel": PlatformWheel},
)
EOF2

  (cd "$wheel_stage" && python3 setup.py bdist_wheel --python-tag py3 --plat-name "$platform" --dist-dir "$BUILD_DIR")
}
