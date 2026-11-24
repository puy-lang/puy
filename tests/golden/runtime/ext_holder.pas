// puy-test: test-ext
// A Pascal value a Rust value holds lives as long as the Rust value, and is let go of after the
// Rust value is dropped (`docs/spec/extensions.md` 5.4): the closure the holder keeps, and the
// instance the closure captured, are freed when the holder is.
program ExtHolder;

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

procedure Run;
var
  H: THolder;
begin
  H := MakeHolder;
  WriteLn(H.Call(10));
  WriteLn(H.Call(20));
end;

begin
  Run;
  WriteLn('after Run');
end.
