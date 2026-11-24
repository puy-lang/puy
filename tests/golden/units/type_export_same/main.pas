program Main;

// Two units exporting the same type under the same name produce a single candidate.
uses Left, Right;

var
  Index: TIndex;
begin
  Index := Default(TIndex);
  WriteLn(Index, ' ', TypeInfo(TIndex) = TypeInfo(Integer));
end.
