program InheritedPropertyErrors;

// Diagnostics of `inherited P` (objects.md 2.4, 2.6): E0700 for a name the parent has as a field
// or not at all, E0704 for `inherited [K]` (with the note naming the default array property) and
// for arguments after a property, E0707 for a write to a read-only property and a read of a
// write-only one, E0706 for a `var` argument.

type
  TBase = class
  protected
    FValue: Integer;
    procedure SetOnly(X: Integer);
    function GetItem(I: Integer): Integer;
  public
    property Only: Integer write SetOnly;
    property Value: Integer read FValue;
    property Items[I: Integer]: Integer read GetItem; default;
  end;

  TChild = class(TBase)
  public
    procedure Run;
  end;

procedure TBase.SetOnly(X: Integer);
begin
end;

function TBase.GetItem(I: Integer): Integer;
begin
  Result := I;
end;

procedure Bump(var X: Integer);
begin
end;

procedure TChild.Run;
begin
  WriteLn(inherited FValue);
  WriteLn(inherited Missing);
  WriteLn(inherited [1]);
  WriteLn(inherited Value(1));
  inherited Value := 1;
  WriteLn(inherited Only);
  Bump(inherited Value);
end;

begin
end.
