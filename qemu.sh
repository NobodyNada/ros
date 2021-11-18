#!/bin/bash
set -e

script="$0"
function usage() {
    echo "Usage: $script [options] executable" >&2
    echo "Options:" >&2
    echo "    -d: launch a debugger" >&2
    echo "    -n: disable graphical output" >&2
    echo "    -q: pass an option through to qemu" >&2
    exit 1
}
function exists() {
    command -v "$1" > /dev/null
}

declare -a options
declare -a binaries
file=
debug=0
nox=0

while [ ! -z "$1" ]; do
    while getopts "dnq:" o; do
        case "$o" in
            d)
                debug=1
                ;;
            n)
                nox=1
                ;;
            q)
                shift
                options+=("$OPTARG")
                ;;
        esac
    done
    shift $((OPTIND-1))
    [ -z "$1" ] || { [ -z "$file" ] && file=$1 || binaries+=("$1"); } || usage
    OPTIND=2
done

[ -z "$file" ] && usage

image="$file.img"
./mkimage.sh -i "$file" -o "$image" "${binaries[@]}"

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
