// ===== Sums and statistics of arrays (docs/spec/stdlib/System.Math/README.md chapter 5) =====
// The elements are added from the first to the last.

function Sum(const Data: array of Double): Double;
function SumInt(const Data: array of Integer): Integer;
function SumOfSquares(const Data: array of Double): Double;
// Sum and SumOfSquares at once.
procedure SumsAndSquares(const Data: array of Double; var Sum, SumOfSquares: Double);

// The arithmetic mean. EInvalidArgument for an empty array.
function Mean(const Data: array of Double): Double;
// The smallest / largest element. EInvalidArgument for an empty array; NaN if an element is NaN.
function MinValue(const Data: array of Double): Double;
function MaxValue(const Data: array of Double): Double;
function MinIntValue(const Data: array of Integer): Integer;
function MaxIntValue(const Data: array of Integer): Integer;

// The sample variance and standard deviation (divided by the count - 1; 0 for one element) and
// the population ones (divided by the count). EInvalidArgument for an empty array.
function Variance(const Data: array of Double): Double;
function StdDev(const Data: array of Double): Double;
function PopnVariance(const Data: array of Double): Double;
function PopnStdDev(const Data: array of Double): Double;
// Mean and StdDev at once.
procedure MeanAndStdDev(const Data: array of Double; var Mean, StdDev: Double);

// The Euclidean norm, Sqrt(SumOfSquares(Data)).
function Norm(const Data: array of Double): Double;
