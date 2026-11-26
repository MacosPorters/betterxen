// Point-sampled textured triangle. Vertex ids 0, 1 and 2 are the corners
// (0, 0), (2, 0) and (0, 2). Those positions cover the clip volume and
// extend past it, so every pixel center of an equal-sized viewport is
// inside the triangle. Across that viewport the interpolated uv is
// ((x + 0.5) / width, (y + 0.5) / height). The vertex uv value 2 is
// off screen.

cbuffer SampleBias : register(b0)
{
    float u_bias;
    float v_bias;
    float unused0;
    float unused1;
};

struct VsOut
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

VsOut vs_main(uint id : SV_VertexID)
{
    float2 corner = float2(float((id << 1) & 2), float(id & 2));
    VsOut output;
    output.position = float4(corner.x * 2.0 - 1.0, 1.0 - corner.y * 2.0, 0.0, 1.0);
    output.uv = corner;
    return output;
}

Texture2D tex0 : register(t0);
SamplerState samp0 : register(s0);

float4 ps_main(VsOut input) : SV_Target
{
    return tex0.Sample(samp0, input.uv + float2(u_bias, v_bias));
}
