program FlowErrorReads;

// A variable read only in unreachable statements is W0001; an unassigned read inside an error-typed expression is E2000 (4.4, 4.6)
procedure Unreachable;
var
  Z: Integer;
begin
  var Y := 2;
  Z := 1;
  Exit;
  WriteLn(Y, Z);
end;

procedure ErrorRead;
var
  Q: Integer;
  R: Integer;
begin
  Q := Foo + R;
  WriteLn(Q);
end;

// No E2001 on top when the expression of `Exit(expr)` is an error
function ExitBad: Integer;
begin
  Exit('x');
end;

function ExitUndeclared: Integer;
begin
  Exit(Foo);
end;

// `declared here` for E0710 / E0725
procedure UsesResult;
begin
  Result := 1;
end;

procedure DoIt;
begin
end;

// A constant is not a variable (E0706)
procedure ByRef(var A: Integer);
begin
  A := 1;
end;

// The W0001 fix that removes one name from an identifier list removes only `A, ` (4.6)
procedure TwoNames;
var
  A, B: Integer;
begin
  B := 1;
  WriteLn(B);
end;

const
  K = 5;

var
  X: Integer;

begin
  X := DoIt;
  ByRef(K);
  Dec(K);
  ReadLn(K);
  // A non-type name in a type position (2.1)
  var VV: DoIt;
  var WW: System;
  var ZZ: K;
  TwoNames;
  WriteLn(X);
end.
