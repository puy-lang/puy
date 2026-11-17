program MethodRefDetach;

// A call through a method reference ends normally when the handler unregisters itself during the
// call (objects.md 9.4): the value called is held by the call, so `FOnChange := nil` in the
// handler does not end it, and the next change finds no handler. The same holds for a handler
// that replaces itself with another method.

type
  TChangeEvent = procedure(Value: Integer) of object;

  TModel = class
  private
    FOnChange: TChangeEvent;
    FValue: Integer;
  public
    procedure SetValue(V: Integer);
    property OnChange: TChangeEvent read FOnChange write FOnChange;
  end;

  TView = class
    Model: TModel;
    procedure Once(Value: Integer);
    procedure First(Value: Integer);
    procedure Second(Value: Integer);
  end;

procedure TModel.SetValue(V: Integer);
begin
  FValue := V;
  if Assigned(FOnChange) then
    FOnChange(V)
  else
    WriteLn('no handler for ', V);
end;

procedure TView.Once(Value: Integer);
begin
  WriteLn('once: ', Value);
  Model.OnChange := nil;
  WriteLn('once: still running');
end;

procedure TView.First(Value: Integer);
begin
  WriteLn('first: ', Value);
  Model.OnChange := Second;
  WriteLn('first: done');
end;

procedure TView.Second(Value: Integer);
begin
  WriteLn('second: ', Value);
end;

var
  M: TModel;
  V: TView;
begin
  M := TModel.Create;
  V := TView.Create;
  V.Model := M;
  M.OnChange := V.Once;
  M.SetValue(1);
  M.SetValue(2);
  M.OnChange := V.First;
  M.SetValue(3);
  M.SetValue(4);
  V.Free;
  M.Free;
end.
