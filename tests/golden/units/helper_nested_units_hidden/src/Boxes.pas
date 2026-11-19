unit Boxes;

// Declares a helper nested in the private section of a class.

interface

type
  TBox = class
    V: Integer;
  end;

  TOuter = class
  private
  type
    TP = class helper for TBox
      function Hid: Integer;
    end;
  end;

implementation

function TOuter.TP.Hid: Integer;
begin
  Result := V;
end;

end.
