program TypInfoAmbiguity;

// An unrelated declaration is still ambiguous. Both diagnostics name the original export.
uses System.Rtti, OtherKinds, System.TypInfo;

var
  Visibility: TMemberVisibility;
begin
  WriteLn(Ord(mvPublished));
end.
