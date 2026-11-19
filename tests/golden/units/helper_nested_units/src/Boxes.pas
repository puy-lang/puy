unit Boxes;

// Declares helpers nested in classes: a public one and a private one in the `interface` section,
// and one in a class of the `implementation` section.

interface

type
  TBox = class
    V: Integer;
  end;

  TOuter = class
  type
    TH = class helper for TBox
      function Pick(N: Integer): Integer;
    end;
  private
  type
    TP = class helper for TBox
      function Hid: Integer;
    end;
  end;

function UseHidden(B: TBox): Integer;

implementation

type
  TImpl = class
  type
    TIH = class helper for TBox
      function Inner: Integer;
    end;
  end;

function TOuter.TH.Pick(N: Integer): Integer;
begin
  Result := V + N;
end;

function TOuter.TP.Hid: Integer;
begin
  Result := V * 100;
end;

function TImpl.TIH.Inner: Integer;
begin
  Result := -V;
end;

function UseHidden(B: TBox): Integer;
begin
  Result := B.Hid + B.Inner;
end;

end.
