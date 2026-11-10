program NestedNames;

var
  G: Integer;

// A nested routine's parameters, variables, constants and nested routines may not repeat a local
// of an enclosing routine visible there (E0701; semantics.md 4.9)
procedure Outer(A: Integer);
const
  K = 1;
var
  B: Integer;

  procedure Inner(A: Integer);
  var
    B: string;
  const
    K = 2;
  begin
  end;

  // The name of a nested routine itself
  procedure K;
  begin
  end;

  // A local declared after the nested routine is not visible there, and a global of the same
  // name is hidden as before
  procedure Later;
  var
    C, G: Integer;
  begin
    C := 1;
    G := C;
    WriteLn(G);
  end;

var
  C: Integer;
begin
  B := A;
  C := B;
  WriteLn(C);
  Inner(1);
  Later;
end;

// `Result` in a nested procedure of a procedure (E0710), writing a `const` parameter of an
// enclosing routine (E0707), and a variable of an enclosing routine as a loop variable (E0722)
procedure P(const S: string);
var
  I: Integer;

  procedure Q;
  begin
    Result := 1;
    S := 'x';
    for I := 1 to 2 do
      WriteLn(I);
  end;

begin
  Q;
end;

// The name of an enclosing function written alone is a recursive call needing parentheses
// (E0704; semantics.md 4.3, 4.9). `Result` names its result only in a nested procedure
function F: Integer;

  procedure InProc;
  var
    X: Integer;
  begin
    X := F;
    WriteLn(X);
  end;

  function InFunc: Integer;
  begin
    Result := F;
  end;

begin
  F := 1;
  InProc;
  WriteLn(InFunc);
end;

// A nested routine may read a variable its enclosing routine uses as a `for` loop variable, but
// writing it would let the loop run on (E0707 at the write; semantics.md 4.9)
procedure Loops;
var
  I, J: Integer;

  procedure ShowI;
  begin
    WriteLn(I);
  end;

  procedure SkipJ;
  begin
    J := 100;
  end;

begin
  for I := 1 to 2 do
    ShowI;
  for J := 1 to 10 do
    SkipJ;
end;

begin
  Outer(1);
  P('a');
  WriteLn(F());
  Loops;
end.
