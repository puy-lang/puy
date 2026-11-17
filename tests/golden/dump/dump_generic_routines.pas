program DumpGenericRoutines;

// The `hir` of generic routines and methods (generics.md 8): their bodies as `.routine -` with
// the type parameters after the name, checked with the types as `T`, and a generic method's
// `.method -`. The instantiations the file asks for are shown under `.instance` with their
// bodies and the types replaced, a routine nested in a generic routine copied with it. The
// function table names an instantiation after the generic routine with the number of its
// overload (generics.md 4.1: `DumpGenericRoutines.Max#2<Integer>`), and a generic method of an
// instantiated type after that type (`bytecode`).

type
  TUtil = class
    class function Twice<T>(const A: T): TArray<T>; static;
  end;

  TBox<T> = class
    Value: T;
    function Pair<U>(const Other: U): Integer;
  end;

class function TUtil.Twice<T>(const A: T): TArray<T>;
begin
  Result := [A, A];
end;

function TBox<T>.Pair<U>(const Other: U): Integer;
var
  Copy: U;
begin
  Copy := Other;
  Result := Length(TUtil.Twice<U>(Copy));
end;

function Max(const A, B: Double): Double; overload;
begin
  if A > B then
    Result := A
  else
    Result := B;
end;

function Max<T>(const A, B: T; First: Boolean): T; overload;

  function Second: T;
  begin
    Result := B;
  end;

begin
  if First then
    Result := A
  else
    Result := Second;
end;

var
  B: TBox<string>;
begin
  WriteLn(Max(1.5, 2.5), ' ', Max<Integer>(1, 2, True));
  B := TBox<string>.Create;
  WriteLn(B.Pair<Integer>(3));
  B.Free;
end.
