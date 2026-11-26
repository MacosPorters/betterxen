cbuffer Depth : register(b0) { float z; };
struct V { float4 position : SV_Position; float2 uv : TEXCOORD0; };
V main(uint id : SV_VertexID) { V o; float2 p=float2((id<<1)&2,id&2); o.uv=p; o.position=float4(p*float2(2,-2)+float2(-1,1),z,1); return o; }