unit AnonInit;

interface

implementation

var
  P: TProc;

initialization
  var A: Integer := 0;
  P := procedure begin WriteLn(A) end;
  P();
finalization
  var B: Integer := 0;
  P := procedure begin WriteLn(B) end;
  P();
end.
