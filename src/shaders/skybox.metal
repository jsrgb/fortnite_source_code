#include <metal_stdlib>
#include "shadertypes.h"
using namespace metal;

struct SkyboxUniforms {
    float4x4 view_proj;
};

struct SkyboxVertexIn {
    float3 position [[attribute(0)]];
};

struct SkyboxVSOut {
    float4 position [[position]];
    float3 texCoord;
};

vertex SkyboxVSOut vertex_main(
    SkyboxVertexIn in [[stage_in]],
    constant SkyboxUniforms& uniforms [[buffer(BufferKind_Uniforms)]]
) {
    SkyboxVSOut out;
    // Transform position but use it as direction for cube map sampling
    out.position = uniforms.view_proj * float4(in.position, 1.0);
    // Use the local position as the cube map direction
    out.texCoord = in.position;
    return out;
}

fragment float4 fragment_main(
    SkyboxVSOut in [[stage_in]],
    texturecube<float> cubeTexture [[texture(0)]]
) {
    constexpr sampler cubeSampler(mag_filter::linear, min_filter::linear);
    return cubeTexture.sample(cubeSampler, in.texCoord);
}
