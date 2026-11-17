program RefcountLendRoot;

// A field of an instance of a reference counted class passed to a `var` parameter keeps the
// instance alive for the call (the loan counts it; objects.md 14.3): the call can let go of
// the last other reference, and the value is written back before the instance is destroyed.

type
  TBox = class(TRefObject)
  public
    Text: string;
    destructor Destroy; override;
  end;

var
  Current: TBox;

destructor TBox.Destroy;
begin
  WriteLn('destroy with ', Text);
  inherited Destroy;
end;

procedure Change(var S: string);
begin
  Current := nil;
  S := S + ' changed';
  WriteLn('in the call');
end;

procedure Run;
begin
  Current := TBox.Create;
  Current.Text := 'text';
  Change(Current.Text);
  WriteLn('after the call: ', Assigned(Current));
end;

begin
  Run;
  WriteLn('end');
end.
