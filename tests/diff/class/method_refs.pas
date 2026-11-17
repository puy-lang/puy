program MethodRefs;

// Method references (objects.md 9.3, 9.4): an event of type `procedure(…) of object` is
// registered with `O.M` and called through the field or the property; the method of a virtual
// method is the override of the instance's class when the reference is made; `M` and `Self.M`
// inside a method make a reference to the method of `Self`; a function of object returns a value;
// a class method of the class reference; `Assigned` and `nil` unregister the event.

uses
  System.SysUtils;

type
  TNotifyEvent = procedure(Sender: TObject) of object;
  TFormatEvent = function(N: Integer): string of object;
  TTextFn = function: string of object;

  TButton = class
  private
    FOnClick: TNotifyEvent;
  public
    Caption: string;
    procedure Click;
    property OnClick: TNotifyEvent read FOnClick write FOnClick;
  end;

  TForm = class
    Clicks: Integer;
    Prefix: string;
    procedure ButtonClick(Sender: TObject); virtual;
    function Describe(N: Integer): string;
    procedure Attach(B: TButton);
    class function Kind: string; virtual;
  end;

  TMainForm = class(TForm)
    procedure ButtonClick(Sender: TObject); override;
    class function Kind: string; override;
  end;

  TFormClass = class of TForm;

procedure TButton.Click;
begin
  if Assigned(FOnClick) then
    FOnClick(Self)
  else
    WriteLn(Caption, ': no handler');
end;

procedure TForm.ButtonClick(Sender: TObject);
begin
  Inc(Clicks);
  WriteLn('TForm: ', (Sender as TButton).Caption, ' ', Clicks);
end;

function TForm.Describe(N: Integer): string;
begin
  Result := Prefix + IntToStr(N);
end;

procedure TForm.Attach(B: TButton);
begin
  B.OnClick := ButtonClick;
end;

class function TForm.Kind: string;
begin
  Result := 'form';
end;

procedure TMainForm.ButtonClick(Sender: TObject);
begin
  WriteLn('TMainForm first');
  inherited ButtonClick(Sender);
end;

class function TMainForm.Kind: string;
begin
  Result := 'main form';
end;

var
  Ok, Cancel: TButton;
  Base, Main: TForm;
  Fmt: TFormatEvent;
  Text: TTextFn;
  C: TFormClass;
begin
  Ok := TButton.Create;
  Ok.Caption := 'OK';
  Cancel := TButton.Create;
  Cancel.Caption := 'Cancel';
  Base := TForm.Create;
  Main := TMainForm.Create;
  Ok.OnClick := Base.ButtonClick;
  Ok.Click;
  Ok.Click;
  Main.Attach(Cancel);
  Cancel.Click;
  Ok.OnClick := Cancel.OnClick;
  Ok.Click;
  WriteLn(Base.Clicks, ' ', Main.Clicks);
  Base.Prefix := 'item ';
  Fmt := Base.Describe;
  WriteLn(Fmt(3));
  C := TMainForm;
  Text := C.Kind;
  WriteLn(Text());
  Text := Base.Kind;
  WriteLn(Text());
  Ok.OnClick := nil;
  WriteLn(Assigned(Ok.OnClick), ' ', Assigned(Cancel.OnClick));
  Ok.Click;
  Ok.Free;
  Cancel.Free;
  Base.Free;
  Main.Free;
end.
