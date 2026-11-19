unit Far;

// Declares a helper nested in a class; the program does not use this unit.

interface

uses Boxes;

type
  TFarOuter = class
  type
    TFH = class helper for TBox
      function Away: Integer;
    end;
  end;

implementation

function TFarOuter.TFH.Away: Integer;
begin
  Result := V;
end;

end.
