// A CSS filter's colour functions: one matrix and offset on unpremultiplied
// colour (Filter Effects 1 §10). Compile with `qsb`: see EldurFilter.qml.
#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    mat4 colorMatrix;
    vec4 colorOffset;
};
layout(binding = 1) uniform sampler2D source;

void main() {
    vec4 c = texture(source, qt_TexCoord0);
    vec4 plain = c.a > 0.0 ? vec4(c.rgb / c.a, c.a) : vec4(0.0);
    vec4 o = clamp(colorMatrix * plain + colorOffset, 0.0, 1.0);
    fragColor = vec4(o.rgb * o.a, o.a) * qt_Opacity;
}
