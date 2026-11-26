
ByteAddressBuffer source : register(t0);
RWByteAddressBuffer result : register(u0);
cbuffer Parameters : register(b0) { uint count; uint salt; uint wordStride; };
[numthreads(64,1,1)]
void main(uint3 tid : SV_DispatchThreadID) {
    uint accum=0;
    for(uint i=tid.x; i<count; i+=256) accum ^= source.Load(i*wordStride*4)+salt;
    result.Store(tid.x*4,accum);
}
