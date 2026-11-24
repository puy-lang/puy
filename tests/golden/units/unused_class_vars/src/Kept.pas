// A unit with a `var`, a used `class var` holding a reference-counted object and an unused one.
unit Kept;

interface

type
  TNoisy = class(TRefObject)
    FName: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TKeeper = class
    class var Spare: TNoisy;
    class var Held: TNoisy;
  end;

var
  Counter: Integer;

implementation

constructor TNoisy.Create(const AName: string);
begin
  FName := AName;
end;

destructor TNoisy.Destroy;
begin
  WriteLn('free ', FName);
  inherited;
end;

initialization
  WriteLn('init Kept');
finalization
  WriteLn('fini Kept');
end.
