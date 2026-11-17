/* phase-575 W1 -- a float crossing the C/Rust boundary, so the hard-float
 * ABI (AAPCS-VFP) is exercised in both directions. */
float rs_scale(float x, float k);

float c_scale(float x)
{
    return rs_scale(x, 2.5f) + 1.0f;
}
