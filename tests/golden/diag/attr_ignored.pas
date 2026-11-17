program AttrIgnored;

// `[Ref]` and `[Volatile]` are ignored with W0010 (attributes.md 3), whose fix removes the
// attribute: the whole brackets when it is alone in them, otherwise the attribute with the `,`
// after it, or before it when it is the last (first, middle and last of a list).

type
  MarkAttribute = class(TCustomAttribute)
  end;

procedure Show(const [Ref] S: string; [Volatile] N: Integer);
begin
  WriteLn(S, N);
end;

var
  [Ref, Mark] A: Integer;
  [Mark, Volatile, Mark] B: Integer;
  [Mark, Ref] C: Integer;
begin
  A := 1;
  B := 2;
  C := 3;
  Show('x', A + B + C);
end.
