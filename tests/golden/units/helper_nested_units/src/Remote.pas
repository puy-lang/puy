unit Remote;

// Declares a helper nested in a class; the program reaches the class only through an alias.

interface

uses Boxes;

type
  TRemoteOuter = class
  type
    TRH = class helper for TBox
      function Far: Integer;
    end;
  end;

implementation

function TRemoteOuter.TRH.Far: Integer;
begin
  Result := V * 3;
end;

end.
