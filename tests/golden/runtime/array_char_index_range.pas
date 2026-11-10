program ArrayCharIndexRange;

// The ERangeError of an array indexed by Char shows the index as a character code (types.md 2.2).

type
  TCounts = array['a'..'c'] of Integer;

var
  C: TCounts;
  Ch: Char;
begin
  C := Default(TCounts);
  Ch := 'z';
  C[Ch] := 1;
end.
