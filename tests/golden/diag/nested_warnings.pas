program NestedWarnings;
{$WARN unused-parameter ON}

// A variable or parameter read only by a nested routine is read (W0001, W0017), and a variable a
// nested routine uses gets no W0002 (semantics.md 4.6, 4.9)
procedure Use(A, Unused: Integer);
var
  X, Y, Z: Integer;
  Never: Integer;

  procedure Show;
  begin
    WriteLn(A + X);
  end;

  procedure SetY;
  begin
    Y := 2;
    Never := 3;
  end;

begin
  X := 1;
  Show;
  X := 5;
  Y := 1;
  SetY;
  WriteLn(Y);
  Z := 1;
  Z := 2;
  WriteLn(Z);
end;

begin
  Use(1, 2);
end.
