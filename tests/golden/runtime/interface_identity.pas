program InterfaceIdentity;

// The value of an interface is the instance's handle (objects.md 7.4, 14.7): values of different
// interfaces, of `TObject` and of a class holding the same instance are equal, other instances are
// not, and `nil` is equal only to `nil`. FPC compares interface pointers, which differ between
// interfaces of one instance (delphi-diff.md).

type
  IA = interface
    procedure A;
  end;

  IB = interface
    procedure B;
  end;

  TBoth = class(TRefObject, IA, IB)
    procedure A;
    procedure B;
  end;

procedure TBoth.A;
begin
end;

procedure TBoth.B;
begin
end;

procedure Run;
var
  X, Y: TBoth;
  O: TObject;
  IX: IA;
  JX: IB;
  IY: IA;
begin
  X := TBoth.Create;
  Y := TBoth.Create;
  O := X;
  IX := X;
  JX := X;
  IY := Y;
  WriteLn('IA = IB of one instance: ', IX = JX);
  WriteLn('IA = TObject: ', IX = O, ' ', O = JX);
  WriteLn('IA = class: ', IX = X, ' ', X <> JX);
  WriteLn('other instance: ', IX = IY, ' ', IY = Y, ' ', JX <> IY);
  IY := nil;
  WriteLn('nil: ', IY = nil, ' ', IX = nil, ' ', nil = IY, ' ', IX <> IY);
end;

begin
  Run;
end.
