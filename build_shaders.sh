#!/bin/zsh
xcrun -sdk macosx metal -c src/shaders/pos_uv.metal -o src/shaders/pos_uv.air && xcrun -sdk macosx metallib src/shaders/pos_uv.air -o src/shaders/pos_uv.metallib
