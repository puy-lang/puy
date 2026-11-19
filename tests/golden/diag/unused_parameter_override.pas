program UnusedParameterOverride;

// W0017 does not check the parameters of an `override` method: the overridden method dictates
// them (`semantics.md` 4.6). The virtual method itself, a method that only shares the name, and
// a routine nested in an override are still checked.

{$WARN unused-parameter ON}

type
  TBase = class
    procedure Handle(Sender: TObject; Code: Integer); virtual;
    class function Make(N: Integer): Integer; virtual;
    procedure Plain(X: Integer);
  end;

  TDerived = class(TBase)
    procedure Handle(Sender: TObject; Code: Integer); override;
    class function Make(N: Integer): Integer; override;
    procedure Plain(X: Integer);
  end;

  TBox<T> = class(TBase)
    procedure Handle(Sender: TObject; Code: Integer); override;
  end;

  TAssigned = class(TBase)
    procedure Handle(Sender: TObject; Code: Integer); override;
  end;

procedure TBase.Handle(Sender: TObject; Code: Integer);
begin
end;

class function TBase.Make(N: Integer): Integer;
begin
  Result := 0;
end;

procedure TBase.Plain(X: Integer);
begin
end;

procedure TDerived.Handle(Sender: TObject; Code: Integer);

  procedure Inner(Unused: Integer);
  begin
  end;

begin
  Inner(1);
end;

class function TDerived.Make(N: Integer): Integer;
begin
  Result := 1;
end;

procedure TDerived.Plain(X: Integer);
begin
end;

procedure TBox<T>.Handle(Sender: TObject; Code: Integer);
begin
end;

// An assigned but unread parameter of an override gets W0002, as where W0017 is disabled
procedure TAssigned.Handle(Sender: TObject; Code: Integer);
begin
  Code := 5;
end;

var
  B: TBase;
begin
  B := TDerived.Create;
  B.Handle(nil, 1);
  WriteLn(TDerived.Make(2));
  TDerived(B).Plain(3);
  B.Free;
  B := TBox<Integer>.Create;
  B.Handle(nil, 2);
  B.Free;
  B := TAssigned.Create;
  B.Handle(nil, 3);
  B.Free;
  B := TBase.Create;
  B.Plain(4);
  B.Free;
end.
