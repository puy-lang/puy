program AnonCapturedSelf;

// An anonymous method that captures `Self` of a reference counted class counts it (objects.md
// 9.6, §5.2): the instance outlives its last variable while the anonymous method's value is
// held, and is destroyed when that value is let go of. `Self` of a class managed by hand is not
// counted: the anonymous method can be called while the instance lives. The values are local
// variables (a global one is copied to a temporary held to the end of the routine to be called).

type
  TGreeter = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
    function Greeting: TProc;
  end;

  TPlain = class
    Name: string;
    function Greeting: TProc;
  end;

constructor TGreeter.Create(const AName: string);
begin
  Name := AName;
end;

destructor TGreeter.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited;
end;

function TGreeter.Greeting: TProc;
begin
  Result := procedure
    begin
      WriteLn('hello from ', Name);
    end;
end;

function TPlain.Greeting: TProc;
begin
  Result := procedure
    begin
      WriteLn('plain ', Self.Name);
    end;
end;

procedure Run;
var
  G: TGreeter;
  P: TProc;
  M: TPlain;
begin
  G := TGreeter.Create('g');
  P := G.Greeting();
  G := nil;
  WriteLn('variable cleared');
  P();
  P := nil;
  WriteLn('value cleared');
  M := TPlain.Create;
  M.Name := 'm';
  P := M.Greeting();
  P();
  P := nil;
  M.Free;
end;

begin
  Run;
  WriteLn('end');
end.
