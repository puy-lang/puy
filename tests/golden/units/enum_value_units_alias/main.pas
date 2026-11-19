program Main;

// A unit that publishes an enumeration type under another name (`TK = TKind`): the fix for an
// unqualified value (types.md 8.1) names the type by the public name this file sees, spelled as
// declared, since the type's own name is not visible here.

uses Other;

begin
  WriteLn(Ord(kTwo))
end.
