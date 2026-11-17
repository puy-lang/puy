// Attributes at the top level of a script (attributes.md 1.3): before `procedure` they belong to
// the routine; a statement starting with `[` stays a set constructor (E0402). A `var` section
// ends when what follows the attributes is not a variable, and a parameter takes attributes
// before and after its modifier.

type
  MarkAttribute = class(TCustomAttribute)
  end;

var
  X: Integer;
  [Mark] Y: Integer;
[Mark][Mark]
procedure P(const [Mark] A: Integer; [Mark] var B: Integer);
begin
  B := A;
end;

X := 1;
P(X, Y);
WriteLn(Y);
[1, 2];
