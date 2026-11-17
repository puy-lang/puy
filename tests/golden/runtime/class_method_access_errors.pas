program ClassMethodAccessErrors;

// A method called on `nil` or on a freed instance (objects.md 14.3, 14.4): `CallMethod` checks
// `Self` even though a method that is not virtual does not need the instance's class, so the
// call raises EAccessViolation before the body runs, as `CallVirtual`, `ToString` and
// `ClassName` do. The last call leaves the exception unhandled, for the report and its trace.

type
  TItem = class
    Value: Integer;
    procedure Touch;
    procedure Show; virtual;
  end;

procedure TItem.Touch;
begin
  WriteLn('touched');
end;

procedure TItem.Show;
begin
  WriteLn('shown');
end;

procedure Try1(const What: string; Step: Integer);
var
  A: TItem;
begin
  A := nil;
  try
    if Step >= 4 then
    begin
      A := TItem.Create;
      A.Free;
    end;
    case Step mod 4 of
      0: A.Touch;
      1: A.Show;
      2: WriteLn(A.ToString);
      3: WriteLn(A.ClassName);
    end;
    WriteLn(What, ': no exception');
  except
    on E: Exception do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

procedure Fail;
var
  A: TItem;
begin
  A := TItem.Create;
  A.Free;
  A.Touch;
end;

begin
  Try1('static nil', 0);
  Try1('virtual nil', 1);
  Try1('ToString nil', 2);
  Try1('ClassName nil', 3);
  Try1('static freed', 4);
  Try1('virtual freed', 5);
  Try1('ToString freed', 6);
  Try1('ClassName freed', 7);
  Fail;
end.
