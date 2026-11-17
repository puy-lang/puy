program MethodRefTemporary;

// A method reference whose receiver is a creation expression warns (W0023; objects.md 9.3): the
// new instance of a reference counted class is held by the creation's temporary until the routine
// ends, so the reference works inside the routine and is no longer assigned after it.

type
  TEvent = procedure of object;

  THandler = class(TRefObject)
    destructor Destroy; override;
    procedure Handle;
  end;

destructor THandler.Destroy;
begin
  WriteLn('destroy handler');
  inherited;
end;

procedure THandler.Handle;
begin
  WriteLn('handle');
end;

var
  Saved: TEvent;

procedure Register;
begin
  Saved := THandler.Create.Handle;
  Saved;
  WriteLn('inside: ', Assigned(Saved));
end;

begin
  Register;
  WriteLn('outside: ', Assigned(Saved));
end.
