program NestedValueErrors;

// Diagnostics of nested routines compiled as closures (objects.md 9.7, 9.8): E0751 where such a
// routine uses what cannot be captured (a `var` parameter, the `Result` of the enclosing
// function), whether it is made a value or called from an anonymous method; E2000 for a variable
// read by the routine made a value before it is assigned (`captured here`); W0024 for a nested
// routine of a method that uses `Self` and is stored in a field, also when it holds `Self` only
// through another nested routine an anonymous method in it calls.

type
  TForm = class(TRefObject)
    Caption: string;
    OnClick: TProc;
    procedure Setup;
    procedure SetupIndirect;
  end;

procedure TForm.Setup;
  procedure Click;
  begin
    WriteLn(Caption);
  end;
begin
  OnClick := Click;
end;

procedure TForm.SetupIndirect;
  procedure Show;
  begin
    WriteLn(Caption);
  end;

  procedure Click;
  var
    P: TProc;
  begin
    P := procedure
      begin
        Show;
      end;
    P();
  end;

begin
  OnClick := Click;
end;

procedure ByRef(var N: Integer);
var
  P: TProc;

  procedure Bump;
  begin
    Inc(N);
  end;

begin
  P := Bump;
  P();
end;

function Total: Integer;
var
  P: TProc;

  procedure Add;
  begin
    Result := Result + 1;
  end;

begin
  Result := 0;
  P := procedure
    begin
      Add;
    end;
  P();
end;

procedure Unassigned;
var
  N: Integer;
  P: TProc;

  procedure Show;
  begin
    WriteLn(N);
  end;

begin
  P := Show;
  N := 1;
  P();
end;

begin
end.
