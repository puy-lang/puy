program ClassIsAs;

// `is`, `as` and conversions by type name between classes (objects.md 2.8, 2.9): `is` is true for
// the class and its ancestors, `as` and `TButton(Sender)` give the instance as the descendant
// when it is one, a class reference converts to a descendant's class reference when it points to
// one, and `as` binds tighter than `=` while `is` binds as loosely as `=`.

type
  TControl = class
    Name: string;
    constructor Create(const AName: string);
    function Describe: string; virtual;
  end;
  TButton = class(TControl)
    Caption: string;
    function Describe: string; override;
  end;
  TCheckBox = class(TButton)
    Checked: Boolean;
  end;
  TLabel = class(TControl)
  end;
  TControlClass = class of TControl;
  TButtonClass = class of TButton;

constructor TControl.Create(const AName: string);
begin
  Name := AName;
end;

function TControl.Describe: string;
begin
  Result := 'control ' + Name;
end;

function TButton.Describe: string;
begin
  Result := 'button ' + Name + ' [' + Caption + ']';
end;

procedure Click(Sender: TControl);
begin
  if Sender is TButton then
    WriteLn('clicked ', TButton(Sender).Caption)
  else
    WriteLn('not a button: ', Sender.Name);
end;

procedure Show(C: TControl);
begin
  WriteLn(C.Name, ': control=', C is TControl, ' button=', C is TButton,
    ' checkbox=', C is TCheckBox, ' label=', C is TLabel);
end;

function Make(K: TControlClass; const AName: string): TControl;
begin
  Result := K.Create(AName);
end;

var
  B, B2: TButton;
  C: TControl;
  X: TCheckBox;
  L: TLabel;
  K: TControlClass;
  BK: TButtonClass;
  Count: Integer;
  All: array[0..2] of TControl;
begin
  B := TButton.Create('ok');
  B.Caption := 'OK';
  X := TCheckBox.Create('agree');
  X.Caption := 'I agree';
  L := TLabel.Create('title');
  Show(B);
  Show(X);
  Show(L);
  Click(B);
  Click(X);
  Click(L);

  C := X;
  B2 := C as TButton;
  WriteLn(B2.Caption);
  (C as TCheckBox).Checked := True;
  WriteLn(X.Checked);
  WriteLn((C as TButton).Describe);
  WriteLn(TButton(C).Describe);
  TCheckBox(C).Checked := False;
  WriteLn(X.Checked);
  WriteLn((C as TControl).Name);
  WriteLn(TControl(B).Describe);

  // `as` binds as `*`, `is` as `=`
  WriteLn(C as TButton = B2);
  WriteLn(C is TButton = True);
  WriteLn(not (C is TLabel) and (C is TButton));

  Count := 0;
  All[0] := B;
  All[1] := X;
  All[2] := L;
  for C in All do
    if C is TButton then
      Inc(Count);
  WriteLn(Count);

  K := TCheckBox;
  BK := TButtonClass(K);
  WriteLn(BK.ClassName);
  C := Make(BK, 'made');
  WriteLn(C.ClassName, ' ', C is TCheckBox);
  C.Free;
  K := TButtonClass(BK);
  WriteLn(K.ClassName);

  B.Free;
  X.Free;
  L.Free;
end.
