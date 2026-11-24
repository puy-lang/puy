program Main;

// A type argument to Default must use the alias of the unit exporting the type.
uses Indices as I;

begin
  WriteLn(Default(I.TIndex));
  WriteLn(Default(TIndex));
end.
