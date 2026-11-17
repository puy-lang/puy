program AnonCellFreed;

// A captured variable lives in a cell that the routine and the anonymous methods capturing it
// hold (objects.md 9.6, 14.9): after the routine returns, the cell lives as long as an anonymous
// method holding it, and the value in it (here an instance of a reference counted class) is let
// go of when the last one is. Two anonymous methods capturing the same variable share the cell.

type
  TItem = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TPair = record
    Show, Swap: TProc;
  end;

constructor TItem.Create(const AName: string);
begin
  Name := AName;
end;

destructor TItem.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited;
end;

function MakePair: TPair;
var
  Item: TItem;
begin
  Result := Default(TPair);
  Item := TItem.Create('first');
  Result.Show := procedure
    begin
      WriteLn('item ', Item.Name);
    end;
  Result.Swap := procedure
    begin
      Item := TItem.Create('second');
    end;
  WriteLn('made');
end;

procedure Run;
var
  P: TPair;
begin
  P := MakePair;
  P.Show();
  P.Swap();
  P.Show();
  P.Show := nil;
  WriteLn('show cleared');
  P.Swap := nil;
  WriteLn('swap cleared');
end;

begin
  Run;
  WriteLn('end');
end.
