#include <metal_stdlib>
#include "shadertypes.h"
using namespace metal;

struct Uniforms {
    float4x4 view_proj;
    float4x4 model;
    float time;
};

struct VertexIn {
    float3 position [[attribute(0)]];
    float3 normals [[attribute(1)]];
    float2 texCoord [[attribute(2)]];
};

struct VSOut {
    float4 position [[position]];
    float2 texCoord;
};

vertex VSOut vertex_main(
      VertexIn in [[stage_in]],
      constant Uniforms& uniforms [[buffer(BufferKind_Uniforms)]]
  ) {
      VSOut out;
      out.position =  uniforms.view_proj * uniforms.model * float4(in.position, 1.0);
      out.texCoord = in.texCoord;
      return out;
  }


fragment float4 fragment_main(
    VSOut in [[stage_in]],
    texture2d<float> colorTexture [[texture(0)]]
) {
    constexpr sampler textureSampler(
        mag_filter::linear,
        min_filter::linear,
        mip_filter::linear,
        address::repeat
    );
    return colorTexture.sample(textureSampler, in.texCoord);
}
