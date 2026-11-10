unit Gamma;

interface

var
  Tag: string = 'gamma';

implementation

initialization
  WriteLn('init Gamma ', Tag);
finalization
  WriteLn('fini Gamma ', Tag);
end.
