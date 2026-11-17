program GenericInferenceErrors;

// The errors of type argument inference (generics.md 5.2, 5.4, 9): E0763 for conflicting
// strong candidates (`Max(I, D)`), for conflicting weak candidates (`Sum([1, 2.5])`), for a
// typed constant against a typed value (both strong), for a type argument only in the result
// type, and for one left without a candidate (the note writes the inferred one); E0759 for an
// inferred type argument against a constraint (the note writes it); E0702 when the inferred type
// does not take another argument (a routine with a `var` parameter for a `TFunc`, an argument
// whose shape does not match its parameter's); E0704 for a routine needing arguments written
// without parentheses for `T` (a call there, generics.md 5.2); E0735
// with a note for each generic candidate left out; E0734 between a generic and a non-generic
// candidate, and between two generic candidates.

function Max<T>(const A, B: T): T;
begin
  Result := A;
end;

function Sum<T>(const A: array of T): Integer;
begin
  Result := Length(A);
end;

function Make<T>: T;
begin
  Result := Default(T);
end;

function Pair<K, V>(const A: K; const B: TArray<V>): Integer;
begin
  Result := 0;
end;

procedure Release<T: class>(const X: T);
begin
end;

function Test<T>(const F: TFunc<T, Boolean>; const X: T): Boolean;
begin
  Result := F(X);
end;

function ByRef(var X: Integer): Boolean;
begin
  Result := X > 0;
end;

procedure Put<T>(const L: TArray<T>; const X: T);
begin
end;

function Pick<T>(const A: TArray<T>): Integer; overload;
begin
  Result := 0;
end;

function Pick<T: class>(const A: T; const B: string): Integer; overload;
begin
  Result := 1;
end;

function Pick(const A: string): Integer; overload;
begin
  Result := 2;
end;

function Mix(const A: Integer; const B: Int64): Integer; overload;
begin
  Result := 0;
end;

function Mix<T>(const A: Int64; const B: T): Integer; overload;
begin
  Result := 1;
end;

function Both<T>(const A: T; const B: Integer): Integer; overload;
begin
  Result := 0;
end;

function Both<T>(const A: Integer; const B: T): Integer; overload;
begin
  Result := 1;
end;

const
  Limit: Int64 = 10;

var
  I: Integer;
  D: Double;
begin
  I := 1;
  D := 2;
  WriteLn(Max(I, D));
  WriteLn(Sum([1, 2.5]));
  WriteLn(Max(Limit, I));
  WriteLn(Make);
  WriteLn(Pair(1, nil));
  Release(I);
  WriteLn(Test(ByRef, 1));
  Put(5, 3);
  WriteLn(Max(ByRef, 1));
  WriteLn(Pick(nil));
  WriteLn(Pick(I, 'a'));
  WriteLn(Mix(I, I));
  WriteLn(Both(1, 2));
end.
