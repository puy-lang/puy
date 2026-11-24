// puy-test: test-ext
// A Pascal value a Rust value held in a global variable is let go of while the isolate
// finishes, where the destruction it starts still runs (`docs/spec/extensions.md` 5.4).
program ExtGlobal;

uses TestExt;

type
  TTrace = class(TRefObject)
    destructor Destroy; override;
  end;

destructor TTrace.Destroy;
begin
  WriteLn('trace freed');
  inherited;
end;

var
  G: THolder;

function MakeHolder: THolder;
var
  T: TTrace;
begin
  T := TTrace.Create;
  Result := THolder.Create(function(X: Integer): Integer
    begin
      Result := X + T.RefCount;
    end);
end;

begin
  G := MakeHolder;
  WriteLn(G.Call(10));
  WriteLn('end');
end.
