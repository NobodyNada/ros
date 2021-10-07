#!/bin/bash
set -e

script="$0"
function usage() {
    echo "Usage: $script [options] executable" >&2
    echo "Options:" >&2
    echo "    --debug, -d: launch a debugger" >&2
    echo "    --qemu, -q: pass an option through to qemu"
    exit 1
}
function exists() {
    command -v "$1" > /dev/null
}

declare -a options
file=
debug=0
nox=0

while [ ! -z "$1" ]; do
    case "$1" in
        --debug|-d)
            debug=1
            ;;
        --nox|-n)
            nox=1
            ;;
        --qemu|-q)
            shift
            options+=("$1")
            ;;
        *)
            [ -z "$file" ] && file=$1 || usage
            ;;
    esac
    shift
done
[ -z "$file" ] && usage

image="$file.img"
./mkimage.sh -i "$file" "$image"

options+=(-drive "format=raw,media=disk,index=0,file=$image" -serial mon:stdio -L "$(dirname "$script")/qemu_roms")
if [ $nox -eq 1 ]; then
    options+=(-nographic)
fi

if [ $debug -eq 1 ]; then
    let port=$RANDOM+1024
    options+=(-S -gdb tcp::$port)
    if exists rust-lldb; then
        debugger=(rust-lldb $file -o "gdb-remote $port")
    elif exists lldb; then
        debugger=(lldb $file -o "gdb-remote $port")
    elif exists gdb; then
        debugger=(gdb $file -ex "target remote localhost:$port")
    else
        echo "error: could not find lldb or gdb" >&2
        exit 1
    fi

    setsid qemu-system-i386 "${options[@]}" & "${debugger[@]}"
fi


    
qemu-system-i386 "${options[@]}"
