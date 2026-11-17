program AnonSelfCaptureMore;

// W0024 (objects.md 9.8): the place reached from `Self` can go through an element of an array
// field (`Items[I].OnDone`); passing the anonymous method to a method (`SetHandler(…)`) is not an
// assignment and is not warned about.

type
  TTask = class(TRefObject)
    OnDone: TProc;
    procedure SetHandler(const Value: TProc);
  end;

  TOwner = class(TRefObject)
    Items: TArray<TTask>;
    Caption: string;
    procedure Wire(I: Integer);
  end;

procedure TTask.SetHandler(const Value: TProc);
begin
  OnDone := Value;
end;

procedure TOwner.Wire(I: Integer);
begin
  Items[I].OnDone := procedure begin WriteLn(Caption) end;
  Items[I].SetHandler(procedure begin WriteLn(Caption) end);
end;

begin
end.
