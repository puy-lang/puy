program HintNotHere;

// Hint directives cannot be written on an enumerated value, a parameter or an inline variable
// (attributes.md 6.1): each is a syntax error (E0400) expecting the token of that position.

type
  TE = (A deprecated, B);

procedure Q(X: Integer deprecated);
begin
end;

begin
  var V: Integer := 1 deprecated;
end.
