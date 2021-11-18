#!/bin/bash
set -e

script=$0
function usage() {
    echo "usage: $script [-i|--input infile | --release] [-o outfile] [binaries...]"
    exit 1
}

declare -a binaries
if [ '$PROFILE' = 'release' ]; then
    release=1
else
    release=0
fi
infile=
while [ ! -z "$1" ]; do
    case "$1" in
        --release)
            release=1
            [ -z "$infile" ] || usage
            ;;
        -i|--input)
            shift
            [ -z "$infile" ] && [ ! -z "$1" ] && [ $release -eq 0 ] || usage
            infile=$1
            ;;
        -o|--output)
            shift
            [ -z "$outfile" ] && [ ! -z "$1" ] && outfile=$1 || usage
            ;;
        *)
            binaries+=("$1")
            ;;
    esac
    shift
done

function exists() {
    command -v "$1" > /dev/null
}

! exists objcopy && exists brew && export PATH="$(brew --prefix binutils)/bin:$PATH"
exists objcopy || { echo "error: binutils does not appear to be installed." >&2; exit 1; }

if [ -z "$infile" ]; then
    if [ $release -eq 0 ]; then
        cargo build
        infile="target/i686-none/debug/ros"
    else
        cargo build  --release
        infile="target/i686-none/release/ros"
    fi
fi
[ ! -z "$outfile" ] || outfile="$infile.img"
objcopy -j .boot -j .kernel -O binary "$infile" "$outfile"

for binary in "${binaries[@]}"; do
    if [ -f "$binary" ]; then 
        cat "$binary"
    elif [ -f "$(dirname "$infile")/$binary" ]; then
        cat "$(dirname "$infile")/$binary"
    else
        echo "No such binary '$binary'" >&2
        echo "Try building it 'cargo build' or 'cargo build --release'" >&2
        exit 1
    fi >> "$outfile"
done

echo "Image written to $outfile" >&2
