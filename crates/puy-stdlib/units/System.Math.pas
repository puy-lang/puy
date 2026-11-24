unit System.Math;

// Mathematical routines (docs/spec/stdlib/System.Math/README.md): the minimum and the maximum,
// rounding, powers and logarithms, trigonometric and hyperbolic functions, comparisons of real
// numbers and statistics of arrays. Written from the specification only, not from any other
// implementation. The functions of System (Sin, Cos, ArcTan, Ln, Exp, Sqrt, Int, Frac) and the
// built-in MathFunc1 / MathFunc2 compute the transcendental functions in the VM, the same bits
// on every target (docs/spec/bytecode.md 2.22).
//
// Split as System.Generics.Collections (docs/spec/collections.md 1.2): Name.Interface.inc holds
// the declarations and Name.Implementation.inc the bodies of one group of routines, read only
// when a program names one of them. Basic is the minimum, the maximum, ranges, signs, rounding
// and comparisons; Power the powers and logarithms; Trig the angles, the trigonometric and the
// hyperbolic functions; Stats the sums and statistics of arrays.

interface

uses
  System.SysUtils;

const
  // The smallest positive (subnormal) and the largest finite values of the real types
  MinSingle = 1.4012984643248171e-45;
  MaxSingle = 3.4028234663852886e38;
  MinDouble = 4.9406564584124654e-324;
  MaxDouble = 1.7976931348623157e308;
  // Extended is another name for Double
  MinExtended = MinDouble;
  MaxExtended = MaxDouble;

  NaN = 0.0 / 0.0;
  Infinity = 1.0 / 0.0;
  NegInfinity = -1.0 / 0.0;

  // The values of TValueSign
  NegativeValue = -1;
  ZeroValue = 0;
  PositiveValue = 1;

  // The values of TValueRelationship
  LessThanValue = -1;
  EqualsValue = 0;
  GreaterThanValue = 1;

type
  // The result of Sign
  TValueSign = -1..1;
  // The result of CompareValue (declared in System.Types in Delphi)
  TValueRelationship = -1..1;
  // The digit of SimpleRoundTo and of RoundTo: the power of ten to round to
  TRoundToRange = -37..37;
  TRoundToEXRangeExtended = -20..20;

  // An argument outside the domain of a routine of this unit (an empty array, a range whose
  // minimum is above its maximum)
  EInvalidArgument = class(EMathError);

{$I System.Math/Basic.Interface.inc}

{$I System.Math/Power.Interface.inc}

{$I System.Math/Trig.Interface.inc}

{$I System.Math/Stats.Interface.inc}

implementation

const
  // The numbers of the functions of MathFunc1 / MathFunc2, the MathF64 / MathF64x2 intrinsics
  // (docs/spec/bytecode.md 2.22; bytecode::MATH_* / MATH2_* in puy-core). diff/math checks each
  // function, so a number that drifts from the VM's shows there.
  MathTan = 2;
  MathArcSin = 3;
  MathArcCos = 4;
  MathSinh = 6;
  MathCosh = 7;
  MathTanh = 8;
  MathArcSinh = 9;
  MathArcCosh = 10;
  MathArcTanh = 11;
  MathLog10 = 14;
  MathLog2 = 15;
  MathLnXP1 = 16;
  Math2ArcTan2 = 0;
  Math2Power = 1;
  Math2Hypot = 2;

{$I System.Math/Basic.Implementation.inc}

{$I System.Math/Power.Implementation.inc}

{$I System.Math/Trig.Implementation.inc}

{$I System.Math/Stats.Implementation.inc}

end.
