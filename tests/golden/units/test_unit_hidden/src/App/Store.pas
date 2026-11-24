unit App.Store;

interface

type
  TItem = class
  private
    FHidden: Integer;
  end;

function Stored: Integer;

implementation

const
  StoreOnly = 2;

function Stored: Integer;
begin
  Result := StoreOnly;
end;

end.
