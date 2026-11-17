program StdlibComparers;

// The comparers of System.Generics.Defaults (collections.md 10.1, equality.md chapter 6).
// Default compares with KeyCompare / KeyEquals / KeyHash; Construct calls the routines it is
// given. A type with no default order still gives a Default comparer; its Compare raises
// EInvalidOperation. No W0021 is reported for the reintroduced Equals / GetHashCode (the case has
// no .diag, so it must compile without any diagnostic).

uses System.SysUtils, System.Generics.Defaults;

type
  TVersion = record
    Major, Minor: Integer;
    class operator LessThan(const A, B: TVersion): Boolean;
  end;

  TPoint = record
    X, Y: Integer;
  end;

  TByLength = class(TComparer<string>)
  public
    function Compare(const Left, Right: string): Integer; override;
  end;

class operator TVersion.LessThan(const A, B: TVersion): Boolean;
begin
  Result := (A.Major < B.Major) or ((A.Major = B.Major) and (A.Minor < B.Minor));
end;

function TByLength.Compare(const Left, Right: string): Integer;
begin
  Result := Length(Left) - Length(Right);
end;

function Version(Major, Minor: Integer): TVersion;
begin
  Result := Default(TVersion);
  Result.Major := Major;
  Result.Minor := Minor;
end;

function Point(X, Y: Integer): TPoint;
begin
  Result := Default(TPoint);
  Result.X := X;
  Result.Y := Y;
end;

function Sign(N: Integer): Integer;
begin
  if N < 0 then
    Result := -1
  else if N > 0 then
    Result := 1
  else
    Result := 0;
end;

var
  CI: IComparer<Integer>;
  CS: IComparer<string>;
  CD: IComparer<Double>;
  CV: IComparer<TVersion>;
  CP: IComparer<TPoint>;
  EI: IEqualityComparer<Integer>;
  ES: IEqualityComparer<string>;
  EP: IEqualityComparer<TPoint>;
  NaN: Double;
begin
  // Default: the default order
  CI := TComparer<Integer>.Default;
  WriteLn(Sign(CI.Compare(1, 2)), ' ', CI.Compare(7, 7), ' ', Sign(CI.Compare(-1, -5)));
  CS := TComparer<string>.Default;
  WriteLn(Sign(CS.Compare('apple', 'banana')), ' ', CS.Compare('x', 'x'), ' ',
    Sign(CS.Compare('b', 'B')));
  CD := TComparer<Double>.Default;
  NaN := 0.0;
  NaN := NaN / NaN;
  WriteLn(Sign(CD.Compare(1.5, NaN)), ' ', CD.Compare(NaN, NaN), ' ', CD.Compare(0.0, -0.0));
  CV := TComparer<TVersion>.Default;
  WriteLn(Sign(CV.Compare(Version(1, 2), Version(1, 10))), ' ',
    CV.Compare(Version(2, 0), Version(2, 0)));

  // Construct: the given routine decides
  CI := TComparer<Integer>.Construct(
    function(const Left, Right: Integer): Integer
    begin
      Result := Right - Left;
    end);
  WriteLn(Sign(CI.Compare(1, 2)), ' ', CI.Compare(4, 4));

  // A user comparer is a TComparer<T> like the others
  CS := TByLength.Create;
  WriteLn(Sign(CS.Compare('zz', 'aaa')), ' ', CS.Compare('ab', 'cd'));

  // Default of a type with no default order: asking for it works, comparing raises
  CP := TComparer<TPoint>.Default;
  WriteLn('comparer of TPoint: ', Assigned(CP));
  try
    CP.Compare(Point(1, 2), Point(3, 4));
  except
    on E: EInvalidOperation do
      WriteLn(E.ClassName, ': ', E.Message);
  end;

  // Default equality: KeyEquals / KeyHash
  EI := TEqualityComparer<Integer>.Default;
  WriteLn(EI.Equals(3, 3), ' ', EI.Equals(3, 4), ' ', EI.GetHashCode(3) = KeyHash(3));
  ES := TEqualityComparer<string>.Default;
  WriteLn(ES.Equals('abc', 'abc'), ' ', ES.Equals('abc', 'ABC'), ' ',
    ES.GetHashCode('abc') = KeyHash('abc'));
  EP := TEqualityComparer<TPoint>.Default;
  WriteLn(EP.Equals(Point(1, 2), Point(1, 2)), ' ', EP.Equals(Point(1, 2), Point(2, 1)), ' ',
    EP.GetHashCode(Point(1, 2)) = KeyHash(Point(1, 2)));

  // Construct: the given equality and hash
  EI := TEqualityComparer<Integer>.Construct(
    function(const Left, Right: Integer): Boolean
    begin
      Result := Left mod 10 = Right mod 10;
    end,
    function(const Value: Integer): Int64
    begin
      Result := Value mod 10;
    end);
  WriteLn(EI.Equals(13, 23), ' ', EI.Equals(13, 14), ' ', EI.GetHashCode(42));
end.
