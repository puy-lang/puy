program RefcountTObject;

// A variable of type exactly `TObject` counts an instance of a reference counted class and not
// one of a class managed by hand (objects.md 4.2): the first lives as long as a `TObject`
// variable holds it; copying the handle of a freed instance managed by hand is not an error.

type
  TCounted = class(TRefObject)
  public
    destructor Destroy; override;
  end;

  TManual = class
  public
    destructor Destroy; override;
  end;

destructor TCounted.Destroy;
begin
  WriteLn('destroy counted');
  inherited Destroy;
end;

destructor TManual.Destroy;
begin
  WriteLn('destroy manual');
  inherited Destroy;
end;

procedure Run;
var
  O, P: TObject;
  M: TManual;
begin
  O := TCounted.Create;
  P := O;
  O := nil;
  WriteLn('one reference left: ', Assigned(P), ' ', Assigned(O));
  P := nil;
  WriteLn('none left: ', Assigned(P));
  M := TManual.Create;
  O := M;
  M.Free;
  P := O;
  WriteLn('freed handle copied: ', Assigned(P));
end;

begin
  Run;
  WriteLn('end');
end.
