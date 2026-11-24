// A unit whose only global variables are `class var`s nothing uses.
unit Hidden;

interface

type
  THidden = class
    class var Count: Integer;
    class var Name: string;
  end;

implementation

initialization
  WriteLn('init Hidden');
finalization
  WriteLn('fini Hidden');
end.
