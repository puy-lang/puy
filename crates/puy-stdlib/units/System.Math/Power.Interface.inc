// ===== Powers and logarithms (docs/spec/stdlib/System.Math/README.md chapter 3) =====

// Base raised to Exponent (Power(X, 0) is 1, even for NaN; a negative Base with a fractional
// Exponent is NaN).
function Power(const Base, Exponent: Double): Double;
// Base raised to the integer Exponent by repeated squaring.
function IntPower(const Base: Double; const Exponent: Integer): Double;

// Logarithms to base 10, 2 and Base; Ln(1 + X), exact for small X.
function Log10(const X: Double): Double;
function Log2(const X: Double): Double;
function LogN(const Base, X: Double): Double;
function LnXP1(const X: Double): Double;
