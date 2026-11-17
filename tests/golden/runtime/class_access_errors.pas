program ClassAccessErrors;

// Access through a handle that does not reach a live instance (objects.md 3.2, 14.4): `nil` and
// a freed instance are EAccessViolation with two messages, `Free` of `nil` does nothing while
// `Destroy` of `nil` raises, a second `Free` raises, and a variable copied before `Free` holds
// the freed handle too. A new instance never matches an old handle.

type
  TItem = class
    Value: Integer;
    Text: string;
  end;

procedure Try1(const What: string; Step: Integer);
var
  A, B: TItem;
begin
  A := nil;
  try
    case Step of
      0: WriteLn(A.Value);
      1: A.Value := 1;
      2: A.Free;
      3: A.Destroy;
      4:
        begin
          A := TItem.Create;
          B := A;
          A.Free;
          WriteLn(Assigned(A), ' ', A = B);
          WriteLn(B.Text);
        end;
      5:
        begin
          A := TItem.Create;
          A.Free;
          A.Free;
        end;
      6:
        begin
          A := TItem.Create;
          B := A;
          A.Destroy;
          A := TItem.Create;
          WriteLn(A = B);
          A.Value := 5;
          WriteLn(A.Value);
          A.Free;
          B.Value := 6;
        end;
    end;
    WriteLn(What, ': no exception');
  except
    on E: Exception do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

begin
  Try1('read nil', 0);
  Try1('write nil', 1);
  Try1('Free nil', 2);
  Try1('Destroy nil', 3);
  Try1('read freed', 4);
  Try1('Free twice', 5);
  Try1('write freed', 6);
end.
