program ProcStaticMethod;

// A static class method (`class …; static;`) is a value of a plain procedural type (objects.md
// 9.2): it is assigned by its name through the class and called like a routine, and the same
// value can go to a `reference to` type.

type
  TIntFn = function(X: Integer): Integer;
  TIntRef = reference to function(X: Integer): Integer;

  TMath = class
    class function Twice(X: Integer): Integer; static;
    class function Square(X: Integer): Integer; static;
  end;

class function TMath.Twice(X: Integer): Integer;
begin
  Result := X * 2;
end;

class function TMath.Square(X: Integer): Integer;
begin
  Result := X * X;
end;

var
  F: TIntFn;
  R: TIntRef;
begin
  F := TMath.Twice;
  WriteLn(F(21));
  F := TMath.Square;
  WriteLn(F(7));
  R := F;
  WriteLn(R(5));
  WriteLn(F = TMath.Square, ' ', F = TMath.Twice);
end.
