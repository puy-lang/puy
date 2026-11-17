program MethodRefFreed;

// A method reference does not keep its instance (objects.md 9.3): after `Free`, `Assigned` is
// `False`, the value is equal to `nil`, and calling it is EAccessViolation (`access to a freed
// object`). Making a reference from `nil` or from a freed instance is EAccessViolation when the
// reference is made (14.9). A procedural value that is `nil` is EAccessViolation when called
// (9.4), for each family.

uses System.SysUtils;

type
  TEvent = procedure of object;
  TCompute = function(X: Integer): Integer;

  TWorker = class
    Name: string;
    procedure Work;
  end;

procedure TWorker.Work;
begin
  WriteLn('work ', Name);
end;

var
  W, Gone: TWorker;
  E: TEvent;
  C: TCompute;
begin
  W := TWorker.Create;
  W.Name := 'w';
  E := W.Work;
  E;
  WriteLn(Assigned(E), ' ', E = nil);
  W.Free;
  WriteLn(Assigned(E), ' ', E = nil, ' ', E <> nil);
  try
    E;
  except
    on X: EAccessViolation do WriteLn('call: ', X.Message);
  end;
  Gone := nil;
  try
    E := Gone.Work;
  except
    on X: EAccessViolation do WriteLn('nil: ', X.Message);
  end;
  try
    E := W.Work;
  except
    on X: EAccessViolation do WriteLn('freed: ', X.Message);
  end;
  E := nil;
  try
    E();
  except
    on X: EAccessViolation do WriteLn('nil event: ', X.Message);
  end;
  C := nil;
  try
    WriteLn(C(1));
  except
    on X: EAccessViolation do WriteLn('nil function: ', X.Message);
  end;
end.
