use crate::data::RenderObject;

pub struct World {
    pub objects: Vec<RenderObject>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            objects: Vec::new(),
        }
    }
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_object(&mut self, object: RenderObject) {
        self.objects.push(object);
    }

    #[allow(dead_code)]
    pub fn add_objects(&mut self, objects: impl IntoIterator<Item = RenderObject>) {
        self.objects.extend(objects);
    }

    #[allow(dead_code)]
    pub fn remove_by_tag(&mut self, tag: &str) {
        self.objects.retain(|obj| obj.tag != tag);
    }

    #[allow(dead_code)]
    pub fn remove_by_id(&mut self, id: &str) {
        self.objects.retain(|obj| obj.id.as_deref() != Some(id));
    }

    #[allow(dead_code)]
    pub fn get_by_id(&self, id: &str) -> Option<&RenderObject> {
        self.objects
            .iter()
            .find(|obj| obj.id.as_deref() == Some(id))
    }

    #[allow(dead_code)]
    pub fn get_by_tag(&self, tag: &str) -> impl Iterator<Item = &RenderObject> {
        self.objects.iter().filter(move |obj| obj.tag == tag)
    }

    pub fn get_by_tag_mut(&mut self, tag: &str) -> impl Iterator<Item = &mut RenderObject> {
        self.objects.iter_mut().filter(move |obj| obj.tag == tag)
    }
}
