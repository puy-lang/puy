// Removed features without a case elsewhere: E3002, E3012, E3015, E3016.
program RemovedFeaturesDecls;

type
  TOld = object
    X: Integer;
  end;

threadvar
  Counter: Integer;

var
  V: object X: Integer; end;
  R: TVarRec;
  N: Integer;
begin
  N := 1;
  AcquireExceptionObject;
  ReleaseExceptionObject;
  WriteLn(N);
end.
