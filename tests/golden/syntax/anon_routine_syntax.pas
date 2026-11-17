program AnonRoutineSyntax;

// The syntax of anonymous methods (objects.md 9.1): `ANON_ROUTINE` with the parameters, the
// return type and a `ROUTINE_BODY` that has a `var` section and a nested routine.

type
  TAdd = reference to function(A, B: Integer): Integer;

var
  F: TAdd;
begin
  F := function(A, B: Integer): Integer
    var
      T: Integer;

      function Twice(X: Integer): Integer;
      begin
        Result := X * 2;
      end;

    begin
      T := A + B;
      Result := Twice(T);
    end;
  WriteLn(F(1, 2));
end.
