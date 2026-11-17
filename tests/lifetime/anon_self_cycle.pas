program AnonSelfCycle;

// An anonymous method that captures `Self` and is stored in a field of `Self` makes a cycle
// (objects.md 9.8): the instance holds the anonymous method and the anonymous method holds the
// instance, so neither is freed and the leak check reports them, the closure by the name of its
// function and the cell of `Self` by the variable's name (W0024 warns at the assignment).

type
  TForm = class(TRefObject)
    OnClick: TProc;
    Caption: string;
    constructor Create;
    destructor Destroy; override;
  end;

constructor TForm.Create;
begin
  Caption := 'form';
  OnClick := procedure
    begin
      WriteLn('clicked ', Caption);
    end;
end;

destructor TForm.Destroy;
begin
  WriteLn('never destroyed');
  inherited;
end;

procedure Run;
var
  F: TForm;
begin
  F := TForm.Create;
  F.OnClick();
end;

begin
  Run;
  WriteLn('end');
end.
