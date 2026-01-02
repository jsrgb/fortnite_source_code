#!/bin/zsh
xcrun -sdk macosx metal -c src/pos_uv.metal -o src/pos_uv.air && xcrun -sdk macosx metallib src/pos_uv.air -o src/pos_uv.metallib
