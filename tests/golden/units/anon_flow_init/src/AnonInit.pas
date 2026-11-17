unit AnonInit;

interface

implementation

var
  P: TProc;

initialization
  var A: Integer;
  P := procedure begin WriteLn(A) end;
  P();
finalization
  var B: Integer;
  P := procedure begin WriteLn(B) end;
  P();
end.
