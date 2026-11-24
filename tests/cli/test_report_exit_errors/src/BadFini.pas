// The test body can return normally while a unit raises during finalization.
unit BadFini;

interface

implementation

initialization
finalization
  raise EInvalidOp.Create('during finalization');

end.
