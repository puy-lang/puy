// ===== Angles, trigonometric and hyperbolic functions =====
// (docs/spec/stdlib/System.Math/README.md chapter 4). Angles are in radians. Sin, Cos and
// ArcTan are in System.

function Tan(const X: Double): Double;
function Cotan(const X: Double): Double;
function Cot(const X: Double): Double;
function Secant(const X: Double): Double;
function Sec(const X: Double): Double;
function Cosecant(const X: Double): Double;
function Csc(const X: Double): Double;
// Sin and Cos of Theta at once.
procedure SinCos(const Theta: Double; var Sin, Cos: Double);

function ArcSin(const X: Double): Double;
function ArcCos(const X: Double): Double;
// The angle of the point (X, Y), in -Pi..Pi.
function ArcTan2(const Y, X: Double): Double;
function ArcCot(const X: Double): Double;
function ArcSec(const X: Double): Double;
function ArcCsc(const X: Double): Double;

// Sqrt(X * X + Y * Y) without overflow or underflow on the way.
function Hypot(const X, Y: Double): Double;

function Sinh(const X: Double): Double;
function Cosh(const X: Double): Double;
function Tanh(const X: Double): Double;
function CotH(const X: Double): Double;
function SecH(const X: Double): Double;
function CscH(const X: Double): Double;
function ArcSinh(const X: Double): Double;
function ArcCosh(const X: Double): Double;
function ArcTanh(const X: Double): Double;
function ArcCotH(const X: Double): Double;
function ArcSecH(const X: Double): Double;
function ArcCscH(const X: Double): Double;

// Conversions of angles: degrees (360 in a turn), grads (400), cycles (1) and radians (2 Pi).
function DegToRad(const Degrees: Double): Double;
function RadToDeg(const Radians: Double): Double;
function GradToRad(const Grads: Double): Double;
function RadToGrad(const Radians: Double): Double;
function CycleToRad(const Cycles: Double): Double;
function RadToCycle(const Radians: Double): Double;
