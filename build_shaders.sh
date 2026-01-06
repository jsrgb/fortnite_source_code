#!/bin/zsh
xcrun -sdk macosx metal -c src/shaders/transform.metal -o src/shaders/transform.air && xcrun -sdk macosx metallib src/shaders/transform.air -o src/shaders/transform.metallib
