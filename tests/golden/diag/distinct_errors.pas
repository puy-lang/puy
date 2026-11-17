program DistinctErrors;

// Errors with distinct types (types.md 1.3): E0769 for a type that cannot be the original type
// (with the fix that makes it an alias), the cycle E0727, E0702 for a `var` parameter of
// another type and for values that do not fit (named as written, not by their roots), E0714
// with the distinct type's name (also for unary `-` of a constant, which keeps the type),
// E0734 for two candidates at stage 0.5 (routines and a user's operator), and E0763 for two
// strong candidates of a type argument.

const
  K = 1;

type
  TColor = (Red, Green);
  TDigit = 0..9;
  TPoint = record X: Integer; end;
  TFoo = class end;
  IFoo = interface end;
  TProc1 = procedure;
  TInts = array of Integer;

  TPaint = type TColor;           // E0769: an enumeration
  TSmallDigit = type TDigit;      // E0769: a subrange
  TPoint2 = type TPoint;          // E0769: a record
  TFoo2 = type TFoo;              // E0769: a class
  TFooRef = type TClass;          // E0769: a class reference
  IFoo2 = type IFoo;              // E0769: an interface
  TProc2 = type TProc1;           // E0769: a procedural type
  TInts2 = type TInts;            // E0769: a dynamic array
  TStrs = type TArray<string>;    // E0769: a dynamic array
  TSet2 = type TSysCharSet;       // E0700: no such type
  TNotType = type K;              // E0708: not a type

  TA = type TB;                   // E0727: a cycle
  TB = type TA;

  TUserId = type Int64;
  TOrderId = type Int64;
  TSmall = type Byte;
  TInt = type Integer;
  THtml = type string;

  TTotal = record
    V: Int64;
    class operator Add(const A: TTotal; B: TUserId): TTotal;
    class operator Add(const A: TTotal; B: TOrderId): TTotal;
  end;

class operator TTotal.Add(const A: TTotal; B: TUserId): TTotal;
begin
  Result := A;
end;

class operator TTotal.Add(const A: TTotal; B: TOrderId): TTotal;
begin
  Result := A;
end;

procedure Bump(var N: Int64);
begin
  Inc(N);
end;

procedure Show(Id: TUserId); overload;
begin
end;

procedure Show(Id: TOrderId); overload;
begin
end;

function Pick<T>(A, B: T): T;
type
  TOwn = type T;                  // E0769: a type parameter
begin
  Result := A;
end;

const
  Big: TSmall = 300;              // E0714
  Neg = -TInt(-2147483648);       // E0714: `-` keeps `TInt`

var
  Id: TUserId;
  N: Int64;
begin
  Id := 1;
  N := 2;
  Bump(Id);                       // E0702: `var` takes the same type
  var H: THtml := 5;              // E0702: expected `THtml`
  var I: Integer := H;            // E0702: found `THtml`
  Id := 1.5;                      // E0702: expected `TUserId`
  Show(N);                        // E0734: both at stage 0.5
  var T: TTotal := Default(TTotal);
  T := T + N;                     // E0734: both at stage 0.5
  WriteLn(Pick(Id, N));           // E0763
end.
