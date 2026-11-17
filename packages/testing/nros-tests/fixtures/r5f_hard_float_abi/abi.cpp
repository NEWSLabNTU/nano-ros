// phase-575 W1 -- the C++ half: a float argument through extern "C".
extern "C" float c_scale(float x);

extern "C" float cpp_scale(float x)
{
    return c_scale(x) * 0.5f;
}
