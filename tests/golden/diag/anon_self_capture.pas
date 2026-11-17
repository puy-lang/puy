program AnonSelfCapture;

// W0024 (objects.md 9.8): in a method of a reference counted class, an anonymous method that
// captures `Self` (by `Self.` or by a member written without it) is stored in a place reached
// from `Self`: a field written with `Self.`, a field without it, a field of an object a field
// holds, a property. Nothing is reported for an anonymous method that does not capture `Self`, a
// place reached from a local variable or a parameter, or a class managed by hand.

type
  TButton = class(TRefObject)
    OnClick: TProc;
  end;

  TForm = class(TRefObject)
  private
    FHandler: TProc;
    procedure SetHandler(const Value: TProc);
  public
    Button: TButton;
    Caption: string;
    property Handler: TProc read FHandler write SetHandler;
    procedure Wire(Other: TButton);
  end;

  TPlainForm = class
    OnClick: TProc;
    Caption: string;
    procedure Wire;
  end;

procedure TForm.SetHandler(const Value: TProc);
begin
  FHandler := Value;
end;

procedure TForm.Wire(Other: TButton);
var
  Local: TButton;
begin
  Self.FHandler := procedure begin WriteLn(Self.Caption) end;
  FHandler := procedure begin WriteLn(Caption) end;
  Button.OnClick := procedure begin WriteLn(Caption) end;
  Handler := procedure begin WriteLn(Caption) end;
  FHandler := procedure begin WriteLn('no Self') end;
  Local := TButton.Create;
  Local.OnClick := procedure begin WriteLn(Caption) end;
  Other.OnClick := procedure begin WriteLn(Caption) end;
end;

procedure TPlainForm.Wire;
begin
  OnClick := procedure begin WriteLn(Caption) end;
end;

begin
end.
