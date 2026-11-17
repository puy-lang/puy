program AttrRepeated;

// A language attribute written twice on one declaration is the same as once (attributes.md 3):
// its diagnostics (E0748, W0010, E3018) are reported once, not once per attribute. The names
// are compared ignoring case (`[weak, Weak]`).

type
  TNode = class(TRefObject)
  end;

var
  [weak, weak] A: Integer;
  [Ref, Ref] B: Integer;
  [unsafe][unsafe] C: Integer;
  [weak, Weak] D: Integer;
  [weak] E: TNode;
begin
  A := 1;
  B := 2;
  C := 3;
  D := 4;
  E := nil;
  WriteLn(A + B + C + D);
end.
