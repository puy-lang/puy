program ProcParenCall;

// Parentheses call a procedural value or a routine (objects.md 9.4), in a value position too:
// `G := (MakeG)` stores the result of `MakeG`, and `(O) = nil` compares the result of `O`.
// A conversion to a procedural type reads its argument as a value: `TGetter(G)()` and `TRun(R)()`
// call it, and `S := TRun(R)` copies it.

type
  TGetter = function: Integer;
  TObjFn = function: TObject;
  TRun = reference to procedure;

function Seven: Integer;
begin
  Result := 7;
end;

function MakeG: TGetter;
begin
  Result := Seven;
end;

function NoObject: TObject;
begin
  Result := nil;
end;

var
  G: TGetter;
  O: TObjFn;
  R, S: TRun;
  X: Integer;
begin
  G := (MakeG);
  X := (G);
  WriteLn(X);
  WriteLn((G) + 1);
  O := NoObject;
  WriteLn((O) = nil);
  WriteLn(O = nil);
  WriteLn(TGetter(G)());
  R := procedure begin WriteLn('run') end;
  TRun(R)();
  S := TRun(R);
  S();
end.
